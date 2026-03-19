"""
Command-line interface for unifi-cli.
"""

from __future__ import annotations

import os
import sys
import subprocess
from pathlib import Path


def find_native_binary() -> str:
    """Find the native Rust binary."""
    project_root = Path(__file__).resolve().parent.parent
    target_binary = project_root / "target" / "release" / "unifi-cli"
    if target_binary.exists() and not target_binary.is_dir():
        return str(target_binary)

    if sys.platform == "win32":
        target_binary = project_root / "target" / "release" / "unifi-cli.exe"
        if target_binary.exists() and not target_binary.is_dir():
            return str(target_binary)

    raise FileNotFoundError(
        "Could not find the native unifi-cli binary. "
        "Please ensure it was built with 'cargo build --release'."
    )


def main() -> int:
    """Run the unifi-cli command line tool."""
    try:
        native_binary = find_native_binary()
        args = [native_binary] + sys.argv[1:]

        if sys.platform == "win32":
            completed_process = subprocess.run(args)
            return completed_process.returncode
        else:
            os.execv(native_binary, args)
            return 0
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
