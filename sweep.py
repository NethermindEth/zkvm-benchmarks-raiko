import argparse
from itertools import product
import subprocess


def run_benchmark(
    filename,
    trials,
    programs,
    provers,
    # hashfns,
    shard_sizes,
    block_1,
    block_2,
    _fibonacci_inputs,
):
    option_combinations = product(programs, provers, shard_sizes)  # hashfns)
    for program, prover, shard_size in option_combinations:
        if shard_size != shard_sizes[0]:
            # Only sp1 supports different shard size
            continue

        print(f"Running: {program} {prover} {shard_size}")
        for _ in range(trials):
            cmd = [
                "bash",
                "eval.sh",
                "reth" if program.startswith("reth") else program,
                prover,
                str(shard_size),
                filename,
            ]

            if program == "reth1":
                cmd.append(block_1)
            elif program == "reth2":
                cmd.append(block_2)

            subprocess.run(cmd)


def main():
    parser = argparse.ArgumentParser(
        description="Run benchmarks with various combinations of options."
    )
    parser.add_argument(
        "--filename", default="benchmark", help="Filename for the benchmark"
    )
    parser.add_argument("--trials", type=int, default=1, help="Number of trials to run")
    parser.add_argument(
        "--programs",
        nargs="+",
        default=["loop", "fibonacci", "tendermint", "reth1", "reth2"],
        help="List of programs to benchmark",
        choices=["loop", "fibonacci", "tendermint", "reth1", "reth2"],
    )
    parser.add_argument(
        "--provers",
        nargs="+",
        default=["sp1"],
        help="List of provers to use",
        choices=["sp1", "risc0", "lita", "jolt", "nexus"],
    )
    # parser.add_argument(
    #     "--hashfns",
    #     nargs="+",
    #     default=["poseidon"],
    #     help="List of hash functions to use",
    #     choices=["poseidon"],
    # )
    parser.add_argument(
        "--shard-sizes",
        type=int,
        nargs="+",
        default=[21],
        help="List of shard sizes to use",
    )
    parser.add_argument("--block-1", default="17106222", help="Block number for reth1")
    parser.add_argument("--block-2", default="19409768", help="Block number for reth2")
    parser.add_argument(
        "--fibonacci",
        default=[100, 1000, 10000, 300000],
        help="input for fibonacci",
    )

    args = parser.parse_args()

    run_benchmark(
        args.filename,
        args.trials,
        args.programs,
        args.provers,
        # args.hashfns,
        args.shard_sizes,
        args.block_1,
        args.block_2,
        args.fibonacci,
    )


if __name__ == "__main__":
    main()
