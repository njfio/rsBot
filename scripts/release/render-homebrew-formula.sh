#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "usage: $0 <release-tag> <sha256sums-path> [github-owner/repo]" >&2
  exit 1
fi

release_tag="$1"
checksum_manifest="$2"
repository_slug="${3:-${GITHUB_REPOSITORY:-njfio/Tau}}"

if [[ ! -f "${checksum_manifest}" ]]; then
  echo "error: checksum manifest not found: ${checksum_manifest}" >&2
  exit 1
fi

checksum_for() {
  local artifact_name="$1"
  local checksum

  checksum="$(awk -v artifact="${artifact_name}" '
    $2 == artifact || $2 == ("dist/" artifact) { print $1; exit 0 }
  ' "${checksum_manifest}")"

  if [[ -z "${checksum}" ]]; then
    echo "error: missing checksum entry for ${artifact_name}" >&2
    exit 1
  fi

  if [[ ! "${checksum}" =~ ^[0-9a-fA-F]{64}$ ]]; then
    echo "error: invalid checksum for ${artifact_name}: ${checksum}" >&2
    exit 1
  fi

  printf '%s\n' "${checksum}"
}

linux_amd64_sha="$(checksum_for tau-coding-agent-linux-amd64.tar.gz)"
linux_arm64_sha="$(checksum_for tau-coding-agent-linux-arm64.tar.gz)"
macos_amd64_sha="$(checksum_for tau-coding-agent-macos-amd64.tar.gz)"
macos_arm64_sha="$(checksum_for tau-coding-agent-macos-arm64.tar.gz)"

cat <<EOF
class Tau < Formula
  desc "Tau coding agent"
  homepage "https://github.com/${repository_slug}"
  version "${release_tag#v}"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/${repository_slug}/releases/download/${release_tag}/tau-coding-agent-macos-arm64.tar.gz"
      sha256 "${macos_arm64_sha}"
    else
      url "https://github.com/${repository_slug}/releases/download/${release_tag}/tau-coding-agent-macos-amd64.tar.gz"
      sha256 "${macos_amd64_sha}"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/${repository_slug}/releases/download/${release_tag}/tau-coding-agent-linux-arm64.tar.gz"
      sha256 "${linux_arm64_sha}"
    else
      url "https://github.com/${repository_slug}/releases/download/${release_tag}/tau-coding-agent-linux-amd64.tar.gz"
      sha256 "${linux_amd64_sha}"
    end
  end

  def install
    source_binary =
      if OS.mac?
        Hardware::CPU.arm? ? "tau-coding-agent-macos-arm64" : "tau-coding-agent-macos-amd64"
      else
        Hardware::CPU.arm? ? "tau-coding-agent-linux-arm64" : "tau-coding-agent-linux-amd64"
      end
    cp source_binary, "tau-coding-agent"
    bin.install "tau-coding-agent" => "tau"
  end

  test do
    system "#{bin}/tau", "--help"
  end
end
EOF
