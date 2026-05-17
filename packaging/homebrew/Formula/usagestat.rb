class Usagestat < Formula
  desc "Scriptable CLI for local agent usage data"
  homepage "https://github.com/Hashim-K/usagestat"
  version "1.0.0"
  license "MIT"
  depends_on :linux

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/Hashim-K/usagestat/releases/download/v#{version}/usagestat-linux-aarch64.tar.gz"
      sha256 "d0a16fbbda7e06d7c3347b3ac56ebbe285727343fe4f4b40c90df52bd198e75f"
    else
      url "https://github.com/Hashim-K/usagestat/releases/download/v#{version}/usagestat-linux-x86_64.tar.gz"
      sha256 "53a6c30fc482330b60b889aa5e53dbbdaecef5931f792bf9381ee875f0ba6950"
    end
  end

  def install
    bin.install "usagestat"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/usagestat --version")
  end
end
