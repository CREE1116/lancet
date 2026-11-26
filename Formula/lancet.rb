class Lancet < Formula
  desc "Find, read, and change code and documents — on one coordinate system, at index speed"
  homepage "https://github.com/CREE1116/lancet"
  version "0.1.1"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/CREE1116/lancet.git", branch: "main"
  depends_on "rust" => :build if build.head?

  on_macos do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.1/lancet-v0.1.1-aarch64-apple-darwin.tar.gz"
      sha256 "c1e3bdf8e9310405925b6c51d4d3ae5d78dd474c5fb77f964593cab354908c54"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.1/lancet-v0.1.1-x86_64-apple-darwin.tar.gz"
      sha256 "702d06be374f427274a6c7517f5ee4e443f64fbf00838251c37928d3d62fd2f6"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.1/lancet-v0.1.1-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0737e8ea995a6916155b42fad302310d4766d870bfbdb6a6ddfddd5eadeffcfd"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.1/lancet-v0.1.1-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "5d0e20e460dacb0af05ff876af86deb2fca42b9ac8b33b1c3e198ee1a9ce09e3"
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
