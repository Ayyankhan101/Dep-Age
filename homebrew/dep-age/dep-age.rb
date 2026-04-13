class DepAge < Formula
  desc "Check how old your dependencies are across Cargo.toml, package.json, pyproject.toml, and requirements.txt"
  homepage "https://github.com/Ayyankhan101/Dep-Age"
  url "https://github.com/Ayyankhan101/Dep-Age/releases/download/v0.1.1/dep-age-v0.1.1-x86_64-unknown-linux-gnu.tar.gz"
  sha256 "UPDATE_ME"
  version "0.1.1"
  license "MIT"

  def install
    bin.install "dep-age"
  end

  test do
    system "#{bin}/dep-age", "--version"
  end
end
