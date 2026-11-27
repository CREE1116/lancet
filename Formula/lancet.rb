class Lancet < Formula
  desc "Find, read, and change code and documents — on one coordinate system, at index speed"
  homepage "https://github.com/CREE1116/lancet"
  version "0.1.3"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/CREE1116/lancet.git", branch: "main"
  depends_on "rust" => :build if build.head?

  on_macos do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.3/lancet-v0.1.3-aarch64-apple-darwin.tar.gz"
      sha256 "b14388b89c220d7d86e50d82e0677306c011a09e4d9fd02783385df8b772242e"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.3/lancet-v0.1.3-x86_64-apple-darwin.tar.gz"
      sha256 "07017222074b3dbe244dcf216df3533a95ab0c2ea7897a4df7243dc47b9031f0"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.3/lancet-v0.1.3-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "9d834b4e967921ce9dfac6fb96e7d04d0a8e97708c9e1953be76617957498bb8"
    end
    on_intel do
      url "https://github.com/CREE1116/lancet/releases/download/v0.1.3/lancet-v0.1.3-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "139d5ca98c77fd93e83f0522554694528a51af0f77d8d3088e428a4d2c5e7e70"
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
