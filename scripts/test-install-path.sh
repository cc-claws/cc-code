#!/bin/bash
# scripts/install.sh 的 PATH 写入目标回归测试（#585）。
#
# macOS 的终端默认开 login shell，bash 只读 ~/.bash_profile（其次
# ~/.bash_login / ~/.profile），从不读 ~/.bashrc。修复前安装脚本对 bash
# 一律写 ~/.bashrc，macOS bash 用户装完仍 command not found。
#
# 用本地伪造的 release（tarball + checksums.txt）加一个 stub `curl`，
# 不访问网络，覆盖：macOS+bash 写 .bash_profile 且 login shell 可见、
# 重复运行不重复追加、Linux+bash 仍写 .bashrc、macOS+zsh 仍写 .zshrc。
#
# 用法: bash scripts/test-install-path.sh
set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SH="${ROOT_DIR}/scripts/install.sh"
TAG="npm-v0.0.1"
MAC_ASSET="cc-code-macos-aarch64.tar.gz"
LINUX_ASSET="cc-code-linux-x86_64.tar.gz"

WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

STUB_DIR="${WORK}/stub"
BIN_DIR="${WORK}/bin"
mkdir -p "${STUB_DIR}" "${BIN_DIR}" "${WORK}/payload"

FAILURES=0

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

# $1 = asset 名。tarball 内容与平台无关（解压按 cc-code-* 通配），
# 只需 release.json 与 checksums.txt 里的条目名与所测平台一致。
make_release() {
    local asset="$1"
    printf '#!/bin/sh\necho fake cc-code\n' > "${WORK}/payload/cc-code-macos-aarch64"
    chmod +x "${WORK}/payload/cc-code-macos-aarch64"
    tar -czf "${STUB_DIR}/fake.tar.gz" -C "${WORK}/payload" cc-code-macos-aarch64
    printf '%s  %s\n' "$(sha256 "${STUB_DIR}/fake.tar.gz")" "${asset}" > "${STUB_DIR}/checksums.txt"
    cat > "${STUB_DIR}/release.json" <<EOF
{"tag_name":"${TAG}","assets":[{"name":"${asset}","browser_download_url":"https://github.com/cc-claws/cc-code/releases/download/${TAG}/${asset}"},{"name":"checksums.txt","browser_download_url":"https://github.com/cc-claws/cc-code/releases/download/${TAG}/checksums.txt"}]}
EOF
    echo "{\"tag_name\":\"${TAG}\"}" > "${STUB_DIR}/releases.json"
}

# 本地 stub curl：把 API 与 release 资源映射到 WORK/stub 下的文件
cat > "${BIN_DIR}/curl" <<'EOF'
#!/bin/bash
STUB_DIR="${STUB_DIR:?}"
url=""
out=""
while [ $# -gt 0 ]; do
    case "$1" in
        -o) out="$2"; shift 2 ;;
        -*) shift ;;
        *) url="$1"; shift ;;
    esac
done
case "$url" in
    *api.github.com/repos/cc-claws/cc-code/releases/tags/*)
        src="${STUB_DIR}/release.json" ;;
    *api.github.com/repos/cc-claws/cc-code/releases*)
        src="${STUB_DIR}/releases.json" ;;
    */checksums.txt)
        src="${STUB_DIR}/checksums.txt" ;;
    *.tar.gz)
        src="${STUB_DIR}/fake.tar.gz" ;;
    *)
        echo "stub curl: unexpected url ${url}" >&2
        exit 22 ;;
esac
if [ ! -f "$src" ]; then
    echo "stub curl: 404 ${url}" >&2
    exit 22
fi
if [ -n "$out" ]; then cp "$src" "$out"; else cat "$src"; fi
EOF
chmod +x "${BIN_DIR}/curl"

# 运行安装脚本：$1 = 平台，$2 = SHELL，$3 = HOME（隔离的家目录，用默认安装目录）
run_install() {
    env "PATH=${BIN_DIR}:${PATH}" \
        STUB_DIR="${STUB_DIR}" \
        HOME="$3" \
        SHELL="$2" \
        CC_CODE_INSTALL_PLATFORM="$1" \
        CC_CODE_INSTALL_VERSION="${TAG}" \
        bash "${INSTALL_SH}"
}

check() {
    local label="$1"
    local actual="$2"
    local expected="$3"
    if [[ "${actual}" == "${expected}" ]]; then
        echo "  PASS: ${label}"
    else
        echo "  FAIL: ${label} (got '${actual}', expected '${expected}')"
        FAILURES=$((FAILURES + 1))
    fi
}

count_path_lines() {
    # $1 = profile 文件；不存在时计 0
    if [[ -f "$1" ]]; then
        grep -c "export PATH=" "$1" || true
    else
        echo "0"
    fi
}

echo "== macOS + bash：写 .bash_profile，不建 .bashrc，login shell 可见 =="
make_release "${MAC_ASSET}"
H1="${WORK}/home-mac-bash"
mkdir -p "${H1}"
printf '# pre-existing profile\n' > "${H1}/.bash_profile"
out=$(run_install macos-aarch64 /bin/bash "${H1}" 2>&1)
rc=$?
check "退出码为 0" "${rc}" "0"
check ".bash_profile 有且仅有 1 行 PATH 追加" "$(count_path_lines "${H1}/.bash_profile")" "1"
check ".bash_profile 原有内容保留" "$(grep -c 'pre-existing profile' "${H1}/.bash_profile")" "1"
check "未新建 .bashrc" "$([[ -e "${H1}/.bashrc" ]] && echo yes || echo no)" "no"
found=$(env -i HOME="${H1}" SHELL=/bin/bash PATH=/usr/bin:/bin /bin/bash -l -c 'command -v cc-code' 2>/dev/null)
check "login shell 能找到 cc-code" "${found}" "${H1}/.cc-code/cc-code"

echo "== macOS + bash：重复运行不重复追加 =="
out=$(run_install macos-aarch64 /bin/bash "${H1}" 2>&1)
rc=$?
check "退出码为 0" "${rc}" "0"
check ".bash_profile 仍只有 1 行 PATH 追加" "$(count_path_lines "${H1}/.bash_profile")" "1"

echo "== Linux + bash：仍写 .bashrc，不建 .bash_profile =="
make_release "${LINUX_ASSET}"
H2="${WORK}/home-linux-bash"
mkdir -p "${H2}"
out=$(run_install linux-x86_64 /bin/bash "${H2}" 2>&1)
rc=$?
check "退出码为 0" "${rc}" "0"
check ".bashrc 有且仅有 1 行 PATH 追加" "$(count_path_lines "${H2}/.bashrc")" "1"
check "未新建 .bash_profile" "$([[ -e "${H2}/.bash_profile" ]] && echo yes || echo no)" "no"

echo "== macOS + zsh：仍写 .zshrc =="
make_release "${MAC_ASSET}"
H3="${WORK}/home-mac-zsh"
mkdir -p "${H3}"
out=$(run_install macos-aarch64 /bin/zsh "${H3}" 2>&1)
rc=$?
check "退出码为 0" "${rc}" "0"
check ".zshrc 有且仅有 1 行 PATH 追加" "$(count_path_lines "${H3}/.zshrc")" "1"

echo
if [[ "${FAILURES}" -eq 0 ]]; then
    echo "ALL CHECKS PASSED"
    exit 0
fi
echo "FAILED: ${FAILURES} check(s)"
exit 1
