#!/bin/bash
# scripts/install.sh 的校验逻辑回归测试（#588）。
#
# 用本地伪造的 release（tarball + checksums.txt）加一个 stub `curl`，不访问网络，
# 覆盖：校验通过、摘要不匹配、checksums.txt 缺条目、release 缺 checksums.txt、
# 以及 CC_CODE_INSTALL_DIR / CCCODE_INSTALL_DIR 两个安装目录变量。
#
# 用法: bash scripts/test-install-checksum.sh
set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SH="${ROOT_DIR}/scripts/install.sh"
TAG="npm-v0.0.1"
ASSET="cc-code-macos-aarch64.tar.gz"

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

make_release() {
    printf '#!/bin/sh\necho fake cc-code\n' > "${WORK}/payload/cc-code-macos-aarch64"
    chmod +x "${WORK}/payload/cc-code-macos-aarch64"
    tar -czf "${STUB_DIR}/${ASSET}" -C "${WORK}/payload" cc-code-macos-aarch64
    printf '%s  %s\n' "$(sha256 "${STUB_DIR}/${ASSET}")" "${ASSET}" > "${STUB_DIR}/checksums.txt"

    cat > "${STUB_DIR}/release.json" <<EOF
{"tag_name":"${TAG}","assets":[{"name":"${ASSET}","browser_download_url":"https://github.com/cc-claws/cc-code/releases/download/${TAG}/${ASSET}"},{"name":"checksums.txt","browser_download_url":"https://github.com/cc-claws/cc-code/releases/download/${TAG}/checksums.txt"}]}
EOF
    cat > "${STUB_DIR}/release_no_checksums.json" <<EOF
{"tag_name":"${TAG}","assets":[{"name":"${ASSET}","browser_download_url":"https://github.com/cc-claws/cc-code/releases/download/${TAG}/${ASSET}"}]}
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
        src="${STUB_RELEASE:-${STUB_DIR}/release.json}" ;;
    *api.github.com/repos/cc-claws/cc-code/releases*)
        src="${STUB_DIR}/releases.json" ;;
    */checksums.txt)
        src="${STUB_DIR}/checksums.txt" ;;
    *.tar.gz)
        src="${STUB_DIR}/cc-code-macos-aarch64.tar.gz" ;;
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

# 运行安装脚本：$1 = 安装目录变量名，$2 = 安装目录，$3 = release.json 路径（可选）
run_install() {
    local dir_var="$1"
    local dir_value="$2"
    local release="${3:-${STUB_DIR}/release.json}"
    env "PATH=${BIN_DIR}:${PATH}" \
        STUB_DIR="${STUB_DIR}" \
        STUB_RELEASE="${release}" \
        CC_CODE_INSTALL_PLATFORM="macos-aarch64" \
        CC_CODE_INSTALL_VERSION="${TAG}" \
        CC_CODE_NO_PATH_HINT="1" \
        "${dir_var}=${dir_value}" \
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

installed_binary() {
    if [[ -f "$1/${TAG}/cc-code" ]]; then echo "yes"; else echo "no"; fi
}

make_release

echo "== 校验通过：安装成功且文档名 CC_CODE_INSTALL_DIR 生效 =="
D1="${WORK}/dir1"
out=$(run_install CC_CODE_INSTALL_DIR "${D1}" 2>&1)
rc=$?
check "退出码为 0" "${rc}" "0"
check "输出 Checksum OK" "$(echo "${out}" | grep -c 'Checksum OK')" "1"
check "二进制已落盘" "$(installed_binary "${D1}")" "yes"
check "current-version.txt" "$(cat "${D1}/current-version.txt" 2>/dev/null)" "${TAG}"

echo "== 摘要不匹配：拒绝安装 =="
D2="${WORK}/dir2"
printf 'tampered\n' > "${STUB_DIR}/${ASSET}"
out=$(run_install CC_CODE_INSTALL_DIR "${D2}" 2>&1)
rc=$?
check "退出码为 1" "${rc}" "1"
check "报 Checksum mismatch" "$(echo "${out}" | grep -c 'Checksum mismatch')" "1"
check "二进制未落盘" "$(installed_binary "${D2}")" "no"
check "tarball 已清理" "$(ls "${D2}/${TAG}/"*.tar.gz 2>/dev/null | wc -l | tr -d ' ')" "0"

echo "== checksums.txt 缺条目：拒绝安装 =="
make_release
D3="${WORK}/dir3"
awk '{print $1"  other-asset.tar.gz"}' "${STUB_DIR}/checksums.txt" > "${STUB_DIR}/tmp" \
    && mv "${STUB_DIR}/tmp" "${STUB_DIR}/checksums.txt"
out=$(run_install CC_CODE_INSTALL_DIR "${D3}" 2>&1)
rc=$?
check "退出码为 1" "${rc}" "1"
check "报 no entry" "$(echo "${out}" | grep -c 'has no entry')" "1"
check "二进制未落盘" "$(installed_binary "${D3}")" "no"

echo "== release 缺 checksums.txt：拒绝安装 =="
make_release
D4="${WORK}/dir4"
out=$(run_install CC_CODE_INSTALL_DIR "${D4}" "${STUB_DIR}/release_no_checksums.json" 2>&1)
rc=$?
check "退出码为 1" "${rc}" "1"
check "报 does not publish checksums.txt" "$(echo "${out}" | grep -c 'does not publish checksums.txt')" "1"
check "二进制未落盘" "$(installed_binary "${D4}")" "no"

echo "== 历史变量名 CCCODE_INSTALL_DIR 仍然生效 =="
make_release
D5="${WORK}/dir5"
out=$(run_install CCCODE_INSTALL_DIR "${D5}" 2>&1)
rc=$?
check "退出码为 0" "${rc}" "0"
check "二进制已落盘" "$(installed_binary "${D5}")" "yes"

echo
if [[ "${FAILURES}" -eq 0 ]]; then
    echo "ALL CHECKS PASSED"
    exit 0
fi
echo "FAILED: ${FAILURES} check(s)"
exit 1
