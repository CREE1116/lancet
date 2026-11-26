class Lancet < Formula
  desc "Find, read, and change code and documents — on one coordinate system, at index speed"
  homepage "https://github.com/CREE1116/lancet"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/CREE1116/lancet.git", branch: "main"

  # Prebuilt binary URLs are populated automatically by scripts/update_formula.py
  # after tagging a GitHub Release (`git tag v0.1.0 && git push --tags`).
  # Until the first tagged release is published, `brew install --HEAD CREE1116/lancet/lancet`
  # builds from source cleanly using Homebrew's Rust bottle.
  depends_on "rust" => :build if build.head?

  def install
    if build.head?
      system "cargo", "install", *std_cargo_args
    else
      bin.install "lancet"
      doc.install "README.md", "LICENSE-MIT", "LICENSE-APACHE"
    end
  end

  test do
    assert_match "lancet", shell_output("#{bin}/lancet --version")
  end
end
