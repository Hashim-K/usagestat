class Usagestat < Formula
  desc "Scriptable CLI for local agent usage data"
  homepage "https://github.com/Hashim-K/usagestat"
  version "1.0.2"
  license "MIT"
  depends_on :linux

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/Hashim-K/usagestat/releases/download/v#{version}/usagestat-linux-aarch64.tar.gz"
      sha256 "bd6e75663973f4535494604987b88cb3928b7ead2de8a7bf269b144e80b866c9"
    else
      url "https://github.com/Hashim-K/usagestat/releases/download/v#{version}/usagestat-linux-x86_64.tar.gz"
      sha256 "50aa23ee61a3a19e38ef63d253d1ac7228c511f615ca0f9197553bc3cfc0a530"
    end
  end

  def install
    bin.install "usagestat"
    (share/"usagestat").install "plugins"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/usagestat --version")
  end
end
