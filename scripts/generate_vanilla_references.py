#!/usr/bin/env python3
"""Generate deterministic MC 26.2 Vanilla reference worlds for parity tests."""
from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import time
from pathlib import Path

DEFAULT_SEEDS = [0, 1, -1, 1_234_567_890_123_456_789, -9_223_372_036_854_775_808]


def rewrite_property(path: Path, key: str, value: str) -> None:
    lines = path.read_text(encoding="utf-8").splitlines() if path.exists() else []
    prefix = f"{key}="
    output = [line for line in lines if not line.startswith(prefix)]
    output.append(f"{key}={value}")
    path.write_text("\n".join(output) + "\n", encoding="utf-8")


def generate(server_dir: Path, seed: int, index: int, timeout: float) -> Path:
    world_name = f"parity-vanilla-{index}"
    world_dir = server_dir / world_name
    if world_dir.exists():
        shutil.rmtree(world_dir)
    properties = server_dir / "server.properties"
    rewrite_property(properties, "level-name", world_name)
    rewrite_property(properties, "level-seed", str(seed))
    rewrite_property(properties, "server-port", str(25566 + index))

    java = Path(r"C:\Program Files\Amazon Corretto\jdk25.0.3_9\bin\java.exe")
    process = subprocess.Popen(
        [str(java), "-Xms1G", "-Xmx2G", "-jar", "server.jar", "nogui"],
        cwd=server_dir,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        bufsize=1,
    )
    deadline = time.monotonic() + timeout
    log_lines: list[str] = []
    try:
        assert process.stdout is not None
        while time.monotonic() < deadline:
            line = process.stdout.readline()
            if line:
                log_lines.append(line)
                if "Done (" in line:
                    break
            elif process.poll() is not None:
                raise RuntimeError(f"Vanilla exited before ready ({process.returncode})")
        else:
            raise TimeoutError(f"Vanilla did not become ready within {timeout}s")

        assert process.stdin is not None
        process.stdin.write("stop\n")
        process.stdin.flush()
        process.wait(timeout=timeout)
        if process.returncode != 0:
            raise RuntimeError(f"Vanilla shutdown returned {process.returncode}")
    except Exception:
        process.kill()
        process.wait(timeout=10)
        (server_dir / f"parity-vanilla-{index}.log").write_text("".join(log_lines), encoding="utf-8")
        raise

    (server_dir / f"parity-vanilla-{index}.log").write_text("".join(log_lines), encoding="utf-8")
    if not world_dir.exists():
        raise RuntimeError(f"Expected world was not created: {world_dir}")
    return world_dir


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("server_dir", type=Path)
    parser.add_argument("--timeout", type=float, default=120.0)
    parser.add_argument("--seeds", type=int, nargs="*", default=DEFAULT_SEEDS)
    args = parser.parse_args()
    args.server_dir = args.server_dir.resolve()
    for index, seed in enumerate(args.seeds):
        world = generate(args.server_dir, seed, index, args.timeout)
        print(f"seed={seed} world={world}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
