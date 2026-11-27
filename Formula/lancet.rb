class Lancet < Formula
  desc "Find, read, and change code and documents — on one coordinate system, at index speed"
  homepage "https://github.com/CREE1116/lancet"
  version "0.1.2"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/CREE1116/lancet.git", branch: "main"
  depends_on "rust" => :build if build.head?

  on_macos do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.2/lancet-v0.1.2-aarch64-apple-darwin.tar.gz"
      sha256 "b8c8f1df930f482c7548e9ff487f131a637d889bd9e99509495875a8187e659b"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.2/lancet-v0.1.2-x86_64-apple-darwin.tar.gz"
      sha256 "f92d5b52901aa9d2eb4e2ddd34d1a56f7a2545b5ec75d17a6d06927587ab2d89"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.2/lancet-v0.1.2-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "b4318fde96ec1477972cbae487ebc1f1afab50665975e2155ae22dd138a7c95c"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.2/lancet-v0.1.2-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "9596651b919ea5a20c3fcac6c8ff972b770d09b9673cf2c9982e417f42f54dd1"
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
