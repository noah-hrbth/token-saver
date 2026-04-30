#!/usr/bin/env bash
set -euo pipefail

TAG="${TAG:?TAG env var required (e.g. v0.1.0)}"
: "${GH_TOKEN:?GH_TOKEN env var required (auto-provided by GitHub Actions; used for source-repo reads)}"
: "${TAP_TOKEN:?TAP_TOKEN env var required (PAT with contents:write + pull-requests:write on the tap repo)}"
VERSION="${TAG#v}"
SOURCE_REPO="noah-hrbth/token-saver"
TAP_REPO="noah-hrbth/homebrew-token-saver"

fetch_sha() {
    local target="$1"
    gh release download "$TAG" \
        -R "$SOURCE_REPO" \
        -p "token-saver-${target}.sha256" \
        -O - | awk '{print $1}'
}

SHA_DARWIN_ARM64=$(fetch_sha aarch64-apple-darwin)
SHA_DARWIN_X86_64=$(fetch_sha x86_64-apple-darwin)
SHA_LINUX_X86_64=$(fetch_sha x86_64-unknown-linux-gnu)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

git clone "https://x-access-token:${TAP_TOKEN}@github.com/${TAP_REPO}.git" "$WORK"
cd "$WORK"

BRANCH="bump-${VERSION}"
git checkout -B "$BRANCH"

FORMULA="Formula/token-saver.rb"

python3 - "$VERSION" "$SHA_DARWIN_ARM64" "$SHA_DARWIN_X86_64" "$SHA_LINUX_X86_64" <<'PY'
import re, pathlib, sys

version, sha_arm, sha_x86, sha_linux = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]

p = pathlib.Path("Formula/token-saver.rb")
src = p.read_text()

src = re.sub(r'(\s*version\s+")[^"]+(")', rf'\g<1>{version}\g<2>', src, count=1)

def sub_sha(text, target, sha):
    pattern = rf'(token-saver-{re.escape(target)}\.tar\.gz"\s*\n\s*sha256\s+")[^"]*(")'
    return re.sub(pattern, rf'\g<1>{sha}\g<2>', text)

src = sub_sha(src, "aarch64-apple-darwin", sha_arm)
src = sub_sha(src, "x86_64-apple-darwin", sha_x86)
src = sub_sha(src, "x86_64-unknown-linux-gnu", sha_linux)

p.write_text(src)
print(f"Updated formula to {version}")
PY

git config user.name  "token-saver release bot"
git config user.email "noreply@github.com"
git add "$FORMULA"
git commit -m "token-saver ${VERSION}"
git push origin "$BRANCH"

export GH_TOKEN="$TAP_TOKEN"

gh pr create \
    -R "$TAP_REPO" \
    --title "token-saver ${VERSION}" \
    --body "Automated bump to ${TAG}." \
    --base main \
    --head "$BRANCH"

gh pr merge "$BRANCH" \
    -R "$TAP_REPO" \
    --squash \
    --delete-branch
