use anyhow::{anyhow, Context, Result};
use once_cell::sync::OnceCell;
use raiko_pipeline::{
    parse_metadata, rerun_if_changed, CommandBuilder, GuestMetadata, Metadata, Pipeline,
};
use regex::Regex;
use risc0_binfmt::ProgramBinary;
use risc0_zkos_v1compat::V1COMPAT_ELF;
use risc0_zkp::core::digest::Digest;
use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::Write as _,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

fn main() {
    let pipeline = Risc0Pipeline::new("benchmarks/raiko-risc0", "release");
    pipeline.bins(&["raiko-risc0"], "benchmarks/raiko-risc0/elf");
}

pub static ROOT_DIR: OnceCell<PathBuf> = OnceCell::new();

pub struct Risc0Pipeline {
    pub meta: Metadata,
    pub profile: String,
}

impl Pipeline for Risc0Pipeline {
    fn new(root: &str, profile: &str) -> Self {
        raiko_pipeline::ROOT_DIR.get_or_init(|| PathBuf::from(root));
        Risc0Pipeline {
            meta: parse_metadata(root),
            profile: profile.to_string(),
        }
    }

    fn builder(&self) -> CommandBuilder {
        let mut builder = CommandBuilder::new(&self.meta, "riscv32im-risc0-zkvm-elf", "risc0")
            .rust_flags(&[
                "passes=loweratomic",
                "link-arg=-Ttext=0x00200800",
                "panic=abort",
            ])
            .cc_compiler("gcc".into())
            .c_flags(&[
                "/opt/riscv/bin/riscv32-unknown-elf-gcc",
                "-march=rv32im",
                "-mstrict-align",
                "-falign-functions=2",
            ])
            .custom_args(&["--ignore-rust-version"])
            .custom_env(HashMap::from([(
                "RISC0_FEATURE_bigint2".to_string(),
                "".to_string(),
            )]));
        // Cannot use /.rustup/toolchains/risc0/bin/cargo, use regular cargo
        builder.unset_cargo();
        builder
    }

    fn bins(&self, names: &[&str], dest: &str) {
        rerun_if_changed(&[]);
        let bins = self.meta.get_bins(names);
        let builder = self.builder();
        let executor = builder.build_command(&self.profile, &bins);
        println!(
            "executor: \n   ${:?}\ntargets: \n   {:?}",
            executor.cmd, executor.artifacts
        );
        if executor.artifacts.is_empty() {
            panic!("No artifacts to build");
        }
        let executor = executor.execute().expect("Execution failed");
        // .risc0_placement(dest)
        // .expect("Failed to export Risc0 artifacts");

        self.build_v2_bin(dest, &executor.artifacts);
    }

    fn tests(&self, names: &[&str], dest: &str) {
        rerun_if_changed(&[]);
        let tests = self.meta.get_tests(names);
        let builder = self.builder();
        let executor = builder.test_command(&self.profile, &tests);
        println!(
            "executor: \n   ${:?}\ntargets: \n   {:?}",
            executor.cmd, executor.artifacts
        );
        if executor.artifacts.is_empty() {
            panic!("No artifacts to build");
        }
        executor
            .execute()
            .expect("Execution failed")
            .risc0_placement(dest)
            .expect("Failed to export Risc0 artifacts");
    }
}

impl Risc0Pipeline {
    fn build_v2_bin(&self, dest: &str, artifacts: &[PathBuf]) {
        //pub fn risc0_placement(&self, dest: &str) -> anyhow::Result<()> {

        let root = ROOT_DIR.get().expect("No reference to ROOT_DIR");
        let dest_dir = PathBuf::from(dest);
        if !dest_dir.exists() {
            fs::create_dir_all(&dest_dir).expect("Couldn't create destination directories");
        }

        for src in artifacts {
            let name = file_name(src);

            let mut dest_file =
                fs::File::create(&dest_dir.join(&format!("{}.rs", name.replace('-', "_"))))
                    .expect("Couldn't create destination file");

            let guest = GuestListEntry::build(
                &name,
                root.join(src).to_str().expect("Path is not valid UTF-8"),
            )
            .expect("Couldn't build the guest list entry");

            dest_file
                .write_all(guest.codegen_consts().as_bytes())
                .expect("couldn't write all dest file");

            println!("Write from\n {src:?}\nto\n {dest_file:?}");
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .expect("no filename in path")
        .to_str()
        .expect("filename is non unicode")
        .to_owned()
}

fn extract_path(line: &str) -> Option<PathBuf> {
    let re = Regex::new(r"\(([^)]+)\)").expect("Couldn't create regex");
    re.captures(line)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .map(PathBuf::from)
}

/// Represents an item in the generated list of compiled guest binaries
#[derive(Debug, Clone)]
pub struct GuestListEntry {
    /// The name of the guest binary
    pub name: Cow<'static, str>,

    /// The combined user ELF & kernel ELF guest binary
    pub elf: Cow<'static, [u8]>,

    /// The image id of the guest program.
    pub image_id: Digest,

    /// The path to the ELF binary
    pub path: Cow<'static, str>,
}

impl GuestListEntry {
    /// Builds the [GuestListEntry] by reading the ELF from disk, and calculating the associated
    /// image ID.
    fn build(name: &str, elf_path: &str) -> Result<Self> {
        let mut elf_path = elf_path.to_owned();

        let user_elf = std::fs::read(&elf_path)?;
        let kernel_elf = V1COMPAT_ELF;
        let binary = ProgramBinary::new(&user_elf, &kernel_elf);
        let elf = binary.encode();
        let combined_path = PathBuf::from_str(&(elf_path + ".bin"))?;
        std::fs::write(&combined_path, &elf)?;
        elf_path = combined_path.to_str().unwrap().to_owned();
        let image_id = compute_image_id(&elf, &elf_path)?;

        Ok(Self {
            name: Cow::Owned(name.to_owned()),
            elf: Cow::Owned(elf),
            image_id,
            path: Cow::Owned(elf_path),
        })
    }

    fn codegen_consts(&self) -> String {
        // Quick check for '#' to avoid injection of arbitrary Rust code into the
        // method.rs file. This would not be a serious issue since it would only
        // affect the user that set the path, but it's good to add a check.
        if self.path.contains('#') {
            panic!("method path cannot include #: {}", self.path);
        }

        let upper = self.name.to_uppercase().replace('-', "_");

        let image_id = self.image_id.as_words();
        let elf = format!("include_bytes!({:?})", self.path);

        let mut str = String::new();

        writeln!(&mut str, "pub const {upper}_ELF: &[u8] = {elf};").unwrap();
        writeln!(&mut str, "pub const {upper}_PATH: &str = {:?};", self.path).unwrap();
        writeln!(&mut str, "pub const {upper}_ID: [u32; 8] = {image_id:?};").unwrap();

        str
    }
}

fn r0vm_image_id(path: &str, flag: &str) -> Result<Digest> {
    use hex::FromHex;
    let output = Command::new("r0vm")
        .env_remove("RUST_LOG")
        .args(["--elf", path, flag])
        .output()?;
    if output.status.success() {
        let stdout = String::from_utf8(output.stdout)?;
        let digest = stdout.trim();
        Ok(Digest::from_hex(digest).context("expecting a hex string")?)
    } else {
        let stderr = String::from_utf8(output.stderr)?;
        Err(anyhow!("{stderr}"))
    }
}

fn compute_image_id(elf: &[u8], elf_path: &str) -> Result<Digest> {
    Ok(match r0vm_image_id(elf_path, "--id") {
        Ok(image_id) => image_id,
        Err(_err) => risc0_binfmt::compute_image_id(elf)?,
    })
}
