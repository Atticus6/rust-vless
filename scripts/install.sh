#!/usr/bin/env bash
# rust-vless 一键安装脚本。
#
# 用法：
#   install.sh                     安装最新版本的 master 与 node 二进制
#   install.sh --version 0.1.0     安装指定版本（也可以设置 RUST_VLESS_VERSION）
#   install.sh --systemd           同时写入 systemd 服务（需要 root）
#   install.sh --node-only         只安装子节点（含自升级脚本）
#
# 脚本可被 `curl -fsSL ... | bash -s -- --systemd` 直接执行。
set -euo pipefail

REPO="${RUST_VLESS_REPO:-Atticus6/rust-vless}"
VERSION="${RUST_VLESS_VERSION:-}"
WITH_SYSTEMD=0
NODE_ONLY=0

# 这两个函数在参数解析阶段就会被调用，必须先定义。
log() { printf '[rust-vless] %s\n' "$*"; }
die() { printf '[rust-vless] 错误：%s\n' "$*" >&2; exit 1; }

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || die "--version 需要一个版本号"
      VERSION="$2"
      shift 2
      ;;
    --systemd)
      WITH_SYSTEMD=1
      shift
      ;;
    --node-only)
      NODE_ONLY=1
      shift
      ;;
    --help|-h)
      sed -n '2,12p' "$0" 2>/dev/null || true
      exit 0
      ;;
    *)
      die "未知参数：$1"
      ;;
  esac
done

need() {
  command -v "$1" >/dev/null 2>&1 || die "缺少命令 $1"
}

detect_platform() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$os" in
    linux) os="linux" ;;
    darwin) os="darwin" ;;
    *) die "不支持的系统：$os（Windows 请使用发布包）" ;;
  esac
  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) die "不支持的架构：$arch" ;;
  esac
  # macOS 只发布 arm64；其余组合与 GitHub Actions 产物一致。
  if [ "$os" = "darwin" ] && [ "$arch" = "x86_64" ]; then
    die "macOS x86_64 暂无发布包，请使用源码构建"
  fi
  printf '%s-%s' "$os" "$arch"
}

latest_version() {
  local tag
  tag="$(download_stdout "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"' \
    | head -n1 | sed 's/.*"v\{0,1\}\([^"]*\)"/\1/')"
  [ -n "$tag" ] || die "无法获取最新版本，请用 --version 指定"
  printf '%s' "$tag"
}

download_stdout() {
  if [ "$DOWNLOAD" = "curl -fL" ]; then
    curl -fsSL "$1"
  else
    wget -qO - "$1"
  fi
}

require_download_tool() {
  if command -v curl >/dev/null 2>&1; then
    DOWNLOAD="curl -fL"
  elif command -v wget >/dev/null 2>&1; then
    DOWNLOAD="wget -O -"
  else
    die "需要 curl 或 wget"
  fi
}

download() {
  local url="$1" out="$2"
  if [ "$DOWNLOAD" = "curl -fL" ]; then
    curl -fL "$url" -o "$out"
  else
    wget -qO "$out" "$url"
  fi
}

PLATFORM="$(detect_platform)"
require_download_tool
[ -n "$VERSION" ] || VERSION="$(latest_version)"
log "平台 ${PLATFORM}，版本 ${VERSION}"

OS_NAME="$(uname -s)"
IS_LINUX=0
[ "$OS_NAME" = "Linux" ] && IS_LINUX=1

if [ "$(id -u)" -eq 0 ]; then
  BIN_DIR="/usr/local/bin"
  DATA_DIR="/usr/local/share/rust-vless"
  # 服务与自升级脚本以固定用户运行，节点因此可以替换自己的二进制。
  # macOS 没有 useradd，且本脚本在 macOS 上不使用 systemd。
  if [ "$IS_LINUX" -eq 1 ]; then
    id -u rustvless >/dev/null 2>&1 || useradd --system --home-dir /var/lib/rust-vless \
      --shell /usr/sbin/nologin rustvless
    # 节点使用独立账号与状态目录，避免读写主节点数据库。
    id -u rustvless-node >/dev/null 2>&1 || useradd --system \
      --home-dir /var/lib/rust-vless-node --shell /usr/sbin/nologin rustvless-node
  fi
else
  BIN_DIR="${HOME}/.local/bin"
  DATA_DIR="${HOME}/.local/share/rust-vless"
  mkdir -p "$BIN_DIR" "$DATA_DIR"
  log "非 root 用户：安装到 ${BIN_DIR}，如需 systemd 请用 root 重跑"
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
# 节点二进制放在独立目录，升级脚本对该目录有写权限，也能原子 rename。
NODE_DIR="${DATA_DIR}/node"
mkdir -p "$NODE_DIR"
ARCHIVE="rust-vless-${VERSION}-${PLATFORM}.tar.gz"
URL="https://github.com/${REPO}/releases/download/v${VERSION}/${ARCHIVE}"
log "下载 ${URL}"
download "$URL" "${TMP_DIR}/${ARCHIVE}"
tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "$TMP_DIR"
SRC="${TMP_DIR}/rust-vless-${VERSION}-${PLATFORM}"
[ -d "$SRC" ] || die "发布包结构异常"

install -m 0755 "${SRC}/rust-vless-node" "${NODE_DIR}/rust-vless-node"
ln -sf "${NODE_DIR}/rust-vless-node" "${BIN_DIR}/rust-vless-node"
log "已安装 rust-vless-node 到 ${NODE_DIR}（符号链接 ${BIN_DIR}/rust-vless-node）"
if [ "$NODE_ONLY" -eq 0 ]; then
  install -m 0755 "${SRC}/rust-vless-master" "${BIN_DIR}/rust-vless-master"
  mkdir -p "${DATA_DIR}/web"
  cp -r "${SRC}/web/." "${DATA_DIR}/web/"
  log "已安装 rust-vless-master 到 ${BIN_DIR}（前端在 ${DATA_DIR}/web）"
fi
# 子节点自升级脚本：主节点下发版本号后由节点进程调用。
cat > "${NODE_DIR}/rust-vless-upgrade-node" <<'UPGRADE'
#!/usr/bin/env bash
# 子节点自升级：主节点通过 RUST_VLESS_TARGET_VERSION 传入目标版本。
# 脚本与节点二进制位于同一目录，且该文件归运行用户所有，因此无需 root。
set -euo pipefail
version="${RUST_VLESS_TARGET_VERSION:?缺少 RUST_VLESS_TARGET_VERSION}"
repo="${RUST_VLESS_REPO:-Atticus6/rust-vless}"
bin_dir="$(cd "$(dirname "$0")" && pwd)"
target="${bin_dir}/rust-vless-node"
os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"
case "$arch" in x86_64|amd64) arch="x86_64" ;; aarch64|arm64) arch="aarch64" ;; esac
platform="${os}-${arch}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
url="https://github.com/${repo}/releases/download/v${version}/rust-vless-${version}-${platform}.tar.gz"
if command -v curl >/dev/null 2>&1; then
  curl -fL "$url" -o "${tmp}/pkg.tar.gz"
else
  wget -qO "${tmp}/pkg.tar.gz" "$url"
fi
tar -xzf "${tmp}/pkg.tar.gz" -C "$tmp"
# 先写临时文件，再原子 rename 替换。直接 cp 覆盖正在运行的二进制在 Linux
# 上会 ETXTBSY；rename 只改变目录项，进程继续使用旧 inode。
cp "${tmp}/rust-vless-${version}-${platform}/rust-vless-node" "${target}.new"
chmod 0755 "${target}.new"
mv -f "${target}.new" "$target"
# 结束旧进程让 systemd / Docker 以新二进制重启。PID 由节点通过
# RUST_VLESS_NODE_PID 传入（命令经 sh -c 启动，$PPID 不是节点）。
if [ -n "${RUST_VLESS_NODE_PID:-}" ]; then
  kill -TERM "$RUST_VLESS_NODE_PID" 2>/dev/null || true
fi
UPGRADE
chmod 0755 "${NODE_DIR}/rust-vless-upgrade-node"

if [ "$(id -u)" -eq 0 ] && [ "$IS_LINUX" -eq 1 ]; then
  chown -R rustvless-node:rustvless-node "${NODE_DIR}" 2>/dev/null || true
  chown rustvless:rustvless "${BIN_DIR}/rust-vless-master" 2>/dev/null || true
  chown -R rustvless:rustvless "${DATA_DIR}/web" 2>/dev/null || true
fi

if [ "$WITH_SYSTEMD" -eq 1 ]; then
  [ "$(id -u)" -eq 0 ] || die "--systemd 需要 root 权限"
  command -v systemctl >/dev/null 2>&1 || die "系统没有 systemd"
  mkdir -p /etc/rust-vless /var/lib/rust-vless
  chmod 0750 /etc/rust-vless

  if [ ! -f /etc/rust-vless/master.env ]; then
    cat > /etc/rust-vless/master.env <<EOF
RUST_VLESS_DB=/var/lib/rust-vless/master.db
RUST_VLESS_HTTP_ADDR=0.0.0.0:8080
RUST_VLESS_GRPC_ADDR=0.0.0.0:50051
RUST_VLESS_WEB_DIR=${DATA_DIR}/web
RUST_VLESS_ACCESS_LOG_FILE=/var/lib/rust-vless/access_logs.jsonl
# 首次启动不设置管理员密码：安装码会打印在 journalctl -u rust-vless-master 中。
# 也可以用 RUST_VLESS_SETUP_TOKEN 固定安装码。
EOF
    chmod 0640 /etc/rust-vless/master.env
  fi

  if [ ! -f /etc/rust-vless/node.env ]; then
    cat > /etc/rust-vless/node.env <<EOF
RUST_VLESS_MASTER_GRPC=http://127.0.0.1:50051
RUST_VLESS_NODE_ID=
RUST_VLESS_NODE_TOKEN=
RUST_VLESS_NODE_NAME=node
# 与主节点同机部署时使用不同端口，避免 8080/50051 冲突。
RUST_VLESS_NODE_LISTEN=0.0.0.0:8081
RUST_VLESS_RPC_ADDR=0.0.0.0:50052
RUST_VLESS_UPGRADE_COMMAND=${NODE_DIR}/rust-vless-upgrade-node
EOF
    chmod 0640 /etc/rust-vless/node.env
  fi

  if [ "$NODE_ONLY" -eq 0 ]; then
    cat > /etc/systemd/system/rust-vless-master.service <<EOF
[Unit]
Description=rust-vless master
After=network-online.target

[Service]
Type=simple
EnvironmentFile=/etc/rust-vless/master.env
ExecStart=${BIN_DIR}/rust-vless-master
Restart=always
RestartSec=3
StateDirectory=rust-vless
StateDirectoryMode=0700
User=rustvless
Group=rustvless
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
# ProtectSystem=strict 会把 /tmp 挂成只读；PrivateTmp 提供可写私有 /tmp，
# 供 DuckDB 等组件使用临时文件。
PrivateTmp=yes
ReadWritePaths=/var/lib/rust-vless
[Install]
WantedBy=multi-user.target
EOF
    systemctl daemon-reload
    systemctl enable --now rust-vless-master
    log "已启动 rust-vless-master.service"
  fi

  cat > /etc/systemd/system/rust-vless-node.service <<EOF
[Unit]
Description=rust-vless node
After=network-online.target
[Service]
Type=simple
EnvironmentFile=/etc/rust-vless/node.env
ExecStart=${NODE_DIR}/rust-vless-node
Restart=always
RestartSec=3
StateDirectory=rust-vless-node
User=rustvless-node
Group=rustvless-node
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
# 升级脚本用 mktemp 下载安装包；PrivateTmp 提供可写私有 /tmp。
PrivateTmp=yes
# 自升级脚本需要替换自己的二进制；只放开节点专用目录。
ReadWritePaths=/var/lib/rust-vless-node ${NODE_DIR}
[Install]
WantedBy=multi-user.target
EOF
  systemctl daemon-reload
  log "已写入 rust-vless-node.service；填写 /etc/rust-vless/node.env 后执行 systemctl enable --now rust-vless-node"
fi

cat <<EOF
安装完成。

主节点：${BIN_DIR}/rust-vless-master
子节点：${NODE_DIR}/rust-vless-node（符号链接 ${BIN_DIR}/rust-vless-node）
升级脚本：${NODE_DIR}/rust-vless-upgrade-node

初次使用请启动主节点并打开 Web 地址，安装码见主节点启动日志
（或 RUST_VLESS_SETUP_TOKEN），按页面完成安装。
EOF
