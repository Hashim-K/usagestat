class Usagestat < Formula
  desc "Scriptable CLI for local agent usage data"
  homepage "https://github.com/Hashim-K/usagestat"
  version "0.1.0"
  license "MIT"
  depends_on :linux

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/Hashim-K/usagestat/releases/download/v#{version}/usagestat-linux-aarch64.tar.gz"
      sha256 "REPLACE_WITH_LINUX_AARCH64_TARBALL_SHA256"
    else
      url "https://github.com/Hashim-K/usagestat/releases/download/v#{version}/usagestat-linux-x86_64.tar.gz"
      sha256 "REPLACE_WITH_LINUX_X86_64_TARBALL_SHA256"
    end
  end

  def install
    bin.install "usagestat"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/usagestat --version")
  end
end
