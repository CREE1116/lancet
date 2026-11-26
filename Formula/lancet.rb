class Lancet < Formula
  desc "Find, read, and change code and documents — on one coordinate system, at index speed"
  homepage "https://github.com/CREE1116/lancet"
  version "0.1.0"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/CREE1116/lancet.git", branch: "main"
  depends_on "rust" => :build if build.head?

  on_macos do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.0/lancet-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "fbb4e9cf0101358592f7b32f31ade7483692a7317ddf354688a3989097c072ab"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.0/lancet-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "e488b69a39ba24010db62312f654a930b1b00f2f9a9809546919280b3fb73191"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.0/lancet-v0.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "a0452f038b45e8bbbd083a721655819f51e7449ef117a3c7a7231201e933659a"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.0/lancet-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "846c8d4cd103d65755d4bbe385242ff498a4d8b626ba6d6dbdfd4591f2dbda6f"
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
