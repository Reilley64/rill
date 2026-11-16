"""Fail explicitly for product tasks the scaffold cannot implement yet."""
import sys

if __name__ == "__main__":
    name = sys.argv[1] if len(sys.argv) == 2 else "requested task"
    print(
        f"{name} is not implemented in this scaffold. "
        "No compiler, standard-library, runtime or LSP acceptance is established.",
        file=sys.stderr,
    )
    raise SystemExit(2)
