#!/usr/bin/env bash
# ShapeCraft in a Proxmox VE container: run this on the Proxmox host, as root.
#
#   bash shapecraft-lxc.sh --src shapecraft.tar.gz            # no GPU: software rendering (llvmpipe)
#   bash shapecraft-lxc.sh --src shapecraft.tar.gz --gpu      # the host's /dev/dri render node passed through
#
# It makes an unprivileged Debian 13 container, builds shapecraft-cli inside it (OpenCASCADE from Debian, 7.8),
# and leaves a `shapecraft` user that MCP clients reach over ssh (`ssh shapecraft@HOST shapecraft-cli mcp`).
# The desktop app needs a display, so it isn't built here.
#
# --src takes a tarball of the source tree (from a checkout: `git archive --format=tar.gz -o shapecraft.tar.gz HEAD`)
# or an http(s) URL of one. Everything else has a default; see --help.
set -euo pipefail

CTID=""
SRC=""
GPU=0
HOSTNAME_CT="shapecraft"
STORAGE="local-lvm"
TEMPLATE_STORAGE="local"
BRIDGE="vmbr0"
CORES=4
MEMORY=4096
DISK=16
SSH_KEY=""
TEMPLATE_MATCH="debian-13-standard"

usage() {
  cat <<'EOF'
Usage: shapecraft-lxc.sh --src FILE|URL [options]
  --src FILE|URL        the source tarball (git archive of the repository) or its URL   (required)
  --gpu                 pass the host's GPU render node (/dev/dri/renderD128) into the container
  --render-node PATH    another render node, e.g. /dev/dri/renderD129 (implies --gpu)
  --ctid N              container id (default: the next free one)
  --hostname NAME       (default: shapecraft)
  --storage NAME        root disk storage (default: local-lvm)
  --template-storage N  where templates are kept (default: local)
  --bridge NAME         network bridge, DHCP (default: vmbr0)
  --cores N --memory MB --disk GB    (defaults: 4, 4096, 16; the build wants 4 GB)
  --ssh-key FILE        a public key allowed to log in as the shapecraft user (for MCP over ssh)
EOF
}

RENDER_NODE="/dev/dri/renderD128"
while [ $# -gt 0 ]; do
  case "$1" in
    --src) SRC="${2:?--src needs a value}"; shift 2 ;;
    --gpu) GPU=1; shift ;;
    --render-node) RENDER_NODE="${2:?--render-node needs a value}"; GPU=1; shift 2 ;;
    --ctid) CTID="${2:?--ctid needs a value}"; shift 2 ;;
    --hostname) HOSTNAME_CT="${2:?--hostname needs a value}"; shift 2 ;;
    --storage) STORAGE="${2:?--storage needs a value}"; shift 2 ;;
    --template-storage) TEMPLATE_STORAGE="${2:?--template-storage needs a value}"; shift 2 ;;
    --bridge) BRIDGE="${2:?--bridge needs a value}"; shift 2 ;;
    --cores) CORES="${2:?--cores needs a value}"; shift 2 ;;
    --memory) MEMORY="${2:?--memory needs a value}"; shift 2 ;;
    --disk) DISK="${2:?--disk needs a value}"; shift 2 ;;
    --ssh-key) SSH_KEY="${2:?--ssh-key needs a value}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

# --- Checks before anything is made -------------------------------------------------------------------------------
[ "$(id -u)" -eq 0 ] || die "run this as root on the Proxmox host"
command -v pct >/dev/null && command -v pveam >/dev/null || die "pct and pveam not found: this runs on a Proxmox VE host"
[ -n "$SRC" ] || { usage >&2; die "--src is required"; }
case "$SRC" in
  http://*|https://*) ;;
  *) [ -f "$SRC" ] || die "no such file: $SRC" ;;
esac
for n in "$CORES" "$MEMORY" "$DISK"; do
  case "$n" in ''|*[!0-9]*) die "cores, memory and disk must be whole numbers" ;; esac
done
[ -z "$SSH_KEY" ] || [ -f "$SSH_KEY" ] || die "no such key file: $SSH_KEY"
if [ "$GPU" -eq 1 ]; then
  [ -c "$RENDER_NODE" ] || die "$RENDER_NODE isn't a device on this host (ls /dev/dri to see what there is)"
  # Device passthrough by `devN:` needs Proxmox VE 8.2 or newer.
  ver="$(pveversion | sed -n 's|^pve-manager/\([0-9]*\)\.\([0-9]*\).*|\1 \2|p')"
  read -r major minor <<<"${ver:-0 0}"
  if [ "$major" -lt 8 ] || { [ "$major" -eq 8 ] && [ "$minor" -lt 2 ]; }; then
    die "--gpu needs Proxmox VE 8.2 or newer (this is $(pveversion))"
  fi
fi
if [ -z "$CTID" ]; then
  CTID="$(pvesh get /cluster/nextid)"
fi
pct status "$CTID" >/dev/null 2>&1 && die "container $CTID already exists (pick another with --ctid)"

# --- Template -----------------------------------------------------------------------------------------------------
say "Finding the newest $TEMPLATE_MATCH template"
pveam update >/dev/null
TEMPLATE="$(pveam available --section system | awk '{print $2}' | grep "^${TEMPLATE_MATCH}" | sort -V | tail -n 1)"
[ -n "$TEMPLATE" ] || die "no $TEMPLATE_MATCH template is offered (pveam available --section system)"
if ! pveam list "$TEMPLATE_STORAGE" | grep -q "$TEMPLATE"; then
  say "Downloading $TEMPLATE"
  pveam download "$TEMPLATE_STORAGE" "$TEMPLATE"
fi

# --- Container ----------------------------------------------------------------------------------------------------
say "Creating container $CTID ($HOSTNAME_CT)"
pct create "$CTID" "$TEMPLATE_STORAGE:vztmpl/$TEMPLATE" \
  --hostname "$HOSTNAME_CT" --cores "$CORES" --memory "$MEMORY" --swap 1024 \
  --rootfs "$STORAGE:$DISK" --net0 "name=eth0,bridge=$BRIDGE,ip=dhcp" \
  --unprivileged 1 --features nesting=1 --onboot 1 --ostype debian

in_ct() { pct exec "$CTID" -- bash -c "$1"; }

pct start "$CTID"
say "Waiting for the network"
for _ in $(seq 1 60); do
  in_ct "getent hosts deb.debian.org >/dev/null" && break
  sleep 2
done
in_ct "getent hosts deb.debian.org >/dev/null" || die "container $CTID has no network (check --bridge and DHCP)"

if [ "$GPU" -eq 1 ]; then
  # The node keeps the container's own render group, so the shapecraft user can open it.
  gid="$(in_ct "getent group render | cut -d: -f3" || true)"
  [ -n "$gid" ] || gid="$(in_ct "groupadd --system render && getent group render | cut -d: -f3")"
  say "Passing $RENDER_NODE through (render group $gid)"
  pct stop "$CTID"
  pct set "$CTID" --dev0 "$RENDER_NODE,gid=$gid,mode=0660"
  pct start "$CTID"
  sleep 3
fi

# --- Source -------------------------------------------------------------------------------------------------------
say "Copying the source in"
in_ct "mkdir -p /opt/shapecraft/src"
case "$SRC" in
  http://*|https://*)
    in_ct "apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq curl ca-certificates >/dev/null"
    in_ct "curl -fsSL '$SRC' -o /opt/shapecraft/src.tar.gz"
    ;;
  *) pct push "$CTID" "$SRC" /opt/shapecraft/src.tar.gz ;;
esac
in_ct "tar -xzf /opt/shapecraft/src.tar.gz -C /opt/shapecraft/src && rm /opt/shapecraft/src.tar.gz"
in_ct "test -f /opt/shapecraft/src/Cargo.toml" || die "the tarball has no Cargo.toml at its top (make it with git archive)"

# --- Build --------------------------------------------------------------------------------------------------------
say "Installing build tools and OpenCASCADE"
in_ct "apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
  build-essential pkg-config curl ca-certificates openssh-server fonts-dejavu-core \
  libocct-foundation-dev libocct-modeling-data-dev libocct-modeling-algorithms-dev \
  libocct-data-exchange-dev libocct-ocaf-dev libocct-visualization-dev \
  libvulkan1 mesa-vulkan-drivers vulkan-tools >/dev/null"

in_ct "id shapecraft >/dev/null 2>&1 || useradd --create-home --shell /bin/bash shapecraft"
in_ct "chown -R shapecraft:shapecraft /opt/shapecraft"
if [ "$GPU" -eq 1 ]; then
  in_ct "usermod -aG render,video shapecraft"
fi

say "Installing Rust (for the shapecraft user)"
in_ct "su - shapecraft -c 'curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >/dev/null'"

say "Building shapecraft-cli (a release build takes a while: ~10 minutes on 4 cores)"
in_ct "su - shapecraft -c 'cd /opt/shapecraft/src && ~/.cargo/bin/cargo build --release -p shapecraft-cli'"
in_ct "install -m 0755 /opt/shapecraft/src/target/release/shapecraft-cli /usr/local/bin/shapecraft-cli"

# --- Access and checks --------------------------------------------------------------------------------------------
if [ -n "$SSH_KEY" ]; then
  pct push "$CTID" "$SSH_KEY" /tmp/shapecraft.pub
  in_ct "install -d -m 0700 -o shapecraft -g shapecraft /home/shapecraft/.ssh && \
    cat /tmp/shapecraft.pub >> /home/shapecraft/.ssh/authorized_keys && rm /tmp/shapecraft.pub && \
    chown shapecraft:shapecraft /home/shapecraft/.ssh/authorized_keys && chmod 0600 /home/shapecraft/.ssh/authorized_keys"
fi
in_ct "systemctl enable --now ssh >/dev/null 2>&1 || true"

say "Checking it works"
in_ct "su - shapecraft -c 'shapecraft-cli demo /tmp/shapecraft-demo'" || die "the demo failed inside the container"
if [ "$GPU" -eq 1 ]; then
  in_ct "su - shapecraft -c 'vulkaninfo --summary 2>/dev/null | grep -E \"deviceName|driverName\"'" \
    || say "vulkaninfo found no GPU device: rendering falls back to the CPU"
fi

IP="$(in_ct "hostname -I | awk '{print \$1}'" || true)"
cat <<EOF

ShapeCraft is in container $CTID (${IP:-no address yet}).
  Try it:       pct exec $CTID -- su - shapecraft -c 'shapecraft-cli commands'
  MCP over ssh: {"command": "ssh", "args": ["shapecraft@${IP:-HOST}", "shapecraft-cli", "mcp"]}
EOF
if [ -z "$SSH_KEY" ]; then
  echo "  (add a key for that: rerun with --ssh-key, or put one in /home/shapecraft/.ssh/authorized_keys)"
fi
