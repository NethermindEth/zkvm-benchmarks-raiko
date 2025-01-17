use vergen_git2::{BuildBuilder, Emitter, Git2Builder};

fn main() {
    let mut config = Emitter::default();

    // Add build instructions
    let build_config = BuildBuilder::default()
        .build_timestamp(true)
        .build()
        .unwrap();
    config.add_instructions(&build_config).unwrap();

    // Add git instructions
    let git_config = Git2Builder::default().sha(true).build().unwrap();
    config.add_instructions(&git_config).unwrap();

    config.emit().unwrap();
}
