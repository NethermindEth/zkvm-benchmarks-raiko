use raiko_pipeline::{
    parse_metadata, rerun_if_changed, CommandBuilder, GuestMetadata, Metadata, Pipeline,
};
use std::path::PathBuf;

fn main() {
    let pipeline = Risc0Pipeline::new("benchmarks/raiko-risc0", "release");
    pipeline.bins(&["raiko-risc0"], "benchmarks/raiko-risc0/elf");
}

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
            .custom_args(&["--ignore-rust-version"]);
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
        executor
            .execute()
            .expect("Execution failed")
            .risc0_placement(dest)
            .expect("Failed to export Risc0 artifacts");
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
