class Lancet < Formula
  desc "Find, read, and change code and documents — on one coordinate system, at index speed"
  homepage "https://github.com/CREE1116/lancet"
  version "0.1.4"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/CREE1116/lancet.git", branch: "main"
  depends_on "rust" => :build if build.head?

  on_macos do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.4/lancet-v0.1.4-aarch64-apple-darwin.tar.gz"
      sha256 "54eba00dd343487d37418538687519f81e3874bc0603ea1b84916785d2458292"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.4/lancet-v0.1.4-x86_64-apple-darwin.tar.gz"
      sha256 "c4c7ca0303c47a34f56aa42c6b08f0d9c28a5221de84532f06c7d9312a941c98"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.4/lancet-v0.1.4-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "1aa1ab4c9edd1a34b1bd697416db74b58fd5c86215a5789a6f78646e4bef0f38"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.4/lancet-v0.1.4-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "353d787cfdfa13f92954bc0957e5edcb287d70b88707fe4f64a9b6c63ac0389f"
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
