import sys

from dkdc_md_cli.core import run as _run

__all__ = ["run", "main"]


def run(argv: list[str] | None = None) -> None:
    """Run the dkdc-md-cli CLI with the given arguments."""
    if argv is None:
        argv = sys.argv
    try:
        _run(argv)
    except KeyboardInterrupt:
        sys.exit(130)


def main() -> None:
    """CLI entry point."""
    run()
