class Lancet < Formula
  desc "Find, read, and change code and documents — on one coordinate system, at index speed"
  homepage "https://github.com/CREE1116/lancet"
  version "0.1.5"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/CREE1116/lancet.git", branch: "main"
  depends_on "rust" => :build if build.head?

  on_macos do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.5/lancet-v0.1.5-aarch64-apple-darwin.tar.gz"
      sha256 "27c63c71d8ef44e1cc33e4c6323004a75b6dfdd0da447ea1d3b0d5096748c19a"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.5/lancet-v0.1.5-x86_64-apple-darwin.tar.gz"
      sha256 "f5900e462d6526613e44920f97f5ea33a43225cf1b704e0af3b20cc8541192e6"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.5/lancet-v0.1.5-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "d6c15836e9f5c62b3abdc0be7a01d6c03d6c4d7056814b9e00dcef9afefa14ac"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.5/lancet-v0.1.5-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "86184fa52d1a5a0f10e739431951c85ba2196c1a49c7db5eb4b43c5ba7699440"
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
    assert_match "lancet", shell_output("#{bin}/lancet --version")
  end
end
