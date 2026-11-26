#!/usr/bin/env python3
"""Update Formula/lancet.rb with version + SHA256s from a GitHub Release's SHA256SUMS.txt.

Usage:
    python3 scripts/update_formula.py v0.1.0 path/to/SHA256SUMS.txt
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORMULA = ROOT / "Formula" / "lancet.rb"

TARGETS = {
    "mac_arm": "aarch64-apple-darwin",
    "mac_x86": "x86_64-apple-darwin",
    "lin_x86": "x86_64-unknown-linux-gnu",
    "lin_arm": "aarch64-unknown-linux-gnu",
}


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: update_formula.py <tag> <SHA256SUMS.txt>", file=sys.stderr)
        return 2
    tag = sys.argv[1]
    ver = tag.lstrip("v")
    sums_text = Path(sys.argv[2]).read_text(encoding="utf-8")
    sums: dict[str, str] = {}
    for line in sums_text.splitlines():
        parts = line.split()
        if len(parts) == 2:
            sums[parts[1]] = parts[0]

    def sha_for(target: str) -> str:
        name = f"lancet-{tag}-{target}.tar.gz"
        if name not in sums:
            raise SystemExit(f"missing {name} in SHA256SUMS.txt")
        return sums[name]

    base = f"https://github.com/CREE1116/lancet/releases/download/{tag}"
    rb = f'''class Lancet < Formula
  desc "Find, read, and change code and documents — on one coordinate system, at index speed"
  homepage "https://github.com/CREE1116/lancet"
  version "{ver}"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/CREE1116/lancet.git", branch: "main"
  depends_on "rust" => :build if build.head?

  on_macos do
    on_arm do
      url "{base}/lancet-{tag}-aarch64-apple-darwin.tar.gz"
      sha256 "{sha_for(TARGETS['mac_arm'])}"
    end
    on_intel do
      url "{base}/lancet-{tag}-x86_64-apple-darwin.tar.gz"
      sha256 "{sha_for(TARGETS['mac_x86'])}"
    end
  end

  on_linux do
    on_arm do
      url "{base}/lancet-{tag}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "{sha_for(TARGETS['lin_arm'])}"
    end
    on_intel do
      url "{base}/lancet-{tag}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "{sha_for(TARGETS['lin_x86'])}"
    end
  end

  def install
    if build.head?
      system "cargo", "install", *std_cargo_args
    else
      bin.install "lancet"
      doc.install "README.md", "LICENSE-MIT", "LICENSE-APACHE"
    end
  end

  test do
    assert_match "lancet", shell_output("#{{bin}}/lancet --version")
  end
end
'''
    FORMULA.write_text(rb, encoding="utf-8")
    print(f"Updated {FORMULA} for {tag}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
