import argparse
import json
import os

def init_argparse() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description='Generate random genesis seeds',
    )
    parser.add_argument(
        '-c',
        '--count',
        help='Number of seeds to generate',
        required=True,
        type=int
    )
    parser.add_argument(
        '-o',
        '--out',
        help='Output file',
        required=True,
        type=str
    )
    return parser


def main():
    parser = init_argparse()
    args = parser.parse_args()
    if os.path.exists(args.out):
        print(f"File {args.out} already exists, skipping.")
        return
    # Generate random 32 bytes seeds
    seeds = dict([[f"wallet-seed-{i+1}", os.urandom(32).hex()] for i in range(args.count)])
    with open(args.out, "w") as f:
        json.dump(seeds, f, indent=2)
    print(f"Generated {args.count} seeds to {args.out}")


if __name__ == "__main__":
    main()
