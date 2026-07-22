#!/usr/bin/env bash
# Regenerate the Homebrew formula for a released version and push it to the tap.
# Usage: scripts/update-brew-formula.sh v0.1.0
# Needs: gh (authed with push access to bryceremick/homebrew-tap), curl, shasum.
set -euo pipefail

TAG="${1:?usage: $0 vX.Y.Z}"
VERSION="${TAG#v}"
REPO="bryceremick/rm-comments"
TAP="bryceremick/homebrew-tap"
BASE="https://github.com/$REPO/releases/download/$TAG"

sha() {
  curl -fsSL "$BASE/rm-comments-$TAG-$1.tar.gz" | shasum -a 256 | cut -d' ' -f1
}

echo "Fetching artifact checksums for $TAG..."
SHA_MAC_ARM=$(sha aarch64-apple-darwin)
SHA_MAC_X86=$(sha x86_64-apple-darwin)
SHA_LINUX_ARM=$(sha aarch64-unknown-linux-gnu)
SHA_LINUX_X86=$(sha x86_64-unknown-linux-gnu)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
gh repo clone "$TAP" "$WORK/tap" -- --depth 1
mkdir -p "$WORK/tap/Formula"

cat > "$WORK/tap/Formula/rm-comments.rb" <<EOF
class RmComments < Formula
  desc "Strip all comments from source files, safely, via tree-sitter"
  homepage "https://github.com/$REPO"
  version "$VERSION"
  license "MIT"

  on_macos do
    on_arm do
      url "$BASE/rm-comments-$TAG-aarch64-apple-darwin.tar.gz"
      sha256 "$SHA_MAC_ARM"
    end
    on_intel do
      url "$BASE/rm-comments-$TAG-x86_64-apple-darwin.tar.gz"
      sha256 "$SHA_MAC_X86"
    end
  end

  on_linux do
    on_arm do
      url "$BASE/rm-comments-$TAG-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "$SHA_LINUX_ARM"
    end
    on_intel do
      url "$BASE/rm-comments-$TAG-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "$SHA_LINUX_X86"
    end
  end

  def install
    bin.install "rm-comments"
  end

  test do
    assert_match "Usage", shell_output("#{bin}/rm-comments --help")
  end
end
EOF

cd "$WORK/tap"
if [[ -n "${CI:-}" ]]; then
  git config user.name "github-actions[bot]"
  git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
fi
git add Formula/rm-comments.rb
if git diff --cached --quiet; then
  echo "Formula already up to date for $VERSION — nothing to push."
  exit 0
fi
git commit -m "rm-comments $VERSION"
git push
echo "Formula for $VERSION pushed to $TAP."
