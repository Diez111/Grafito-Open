#!/usr/bin/env bash
# opencode-remote.sh — expone opencode al celular por LAN y/o Tailscale.
#
# Uso:
#   bash scripts/opencode-remote.sh                              # genera clave aleatoria y la muestra
#   OPENCODE_SERVER_PASSWORD='tu-clave' bash scripts/opencode-remote.sh          # clave propia
#   OPENCODE_SERVER_PASSWORD='tu-clave' bash scripts/opencode-remote.sh --web    # abre web UI local además
#
# Una sola vez (con sudo, en tu terminal):
#   sudo ufw allow 4096/tcp comment 'opencode remote'   # solo si usás LAN directa
#   sudo systemctl enable --now tailscaled              # demonio Tailscale
#   tailscale login                                      # abre el navegador, logueate (1 vez)
#   # API key (opcional, solo automatización): https://login.tailscale.com/admin/settings/keys
#   #   -> "Generate access token", guardala como TS_API_KEY en tu entorno, NUNCA en git.
#
# En el celular:
#   - Por Tailscale (recomendado, funciona fuera de casa): instalá la app Tailscale,
#     logueate con la MISMA cuenta, y abrí la URL tailnet que muestra este script.
#   - Por LAN (misma WiFi): http://192.168.0.15:4096 (o http://opencode.local:4096)
# Login web: usuario `opencode` + la clave mostrada. Se puede "instalar" como app (PWA).
#
# HTTPS tailnet (opcional, 1 vez logueado):
#   tailscale serve --bg --set-path / http://127.0.0.1:4096
#   -> https://<tu-host>.<tu-tailnet>.ts.net/ con certificado automático.
#   Para exponer a TODO internet (ojo): tailscale funnel --bg --set-path / http://127.0.0.1:4096
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.opencode/bin:$HOME/.bun/bin:$HOME/.local/bin:$PATH"

MODE="${1:-serve}"
if [[ -z "${OPENCODE_SERVER_PASSWORD:-}" ]]; then
  export OPENCODE_SERVER_PASSWORD
  OPENCODE_SERVER_PASSWORD="$(head -c 18 /dev/urandom | base64 | tr -d '/+=' | head -c 20)"
  GENERATED=1
else
  GENERATED=0
fi
export OPENCODE_SERVER_USERNAME="${OPENCODE_SERVER_USERNAME:-opencode}"

IP="$(hostname -I 2>/dev/null | awk '{print $1}' || true)"
IP="${IP:-192.168.0.15}"
echo "== opencode remoto =="
echo "URL LAN:     http://${IP}:4096   (alt: http://opencode.local:4096)"

# Tailscale: una sola llamada (con timeout: si el demonio está caído tarda ~2s en fallar).
TS_URL_LINE="URL Tailscale: (no logueado) corré: sudo systemctl enable --now tailscaled && tailscale login"
if command -v tailscale >/dev/null 2>&1; then
  TS_JSON="$(timeout 8 tailscale status --json 2>/dev/null || true)"
  TS_LINE="$(echo "$TS_JSON" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(1)
s = d.get("Self", {}) or {}
if d.get("BackendState") != "Running":
    sys.exit(1)
ips = [ip for ip in (s.get("TailscaleIPs") or []) if "." in ip]
dns = (s.get("DNSName") or "").rstrip(".")
out = []
if ips:
    out.append(f"URL Tailscale (http):  http://{ips[0]}:4096   <- usá esta en el celu")
if dns:
    out.append(f"URL Tailscale (https): https://{dns}/   <- requiere: tailscale serve --bg --set-path / http://127.0.0.1:4096")
print("\n".join(out))
sys.exit(0 if out else 1)
' 2>/dev/null || true)"
  [[ -n "${TS_LINE:-}" ]] && TS_URL_LINE="$TS_LINE"
fi
echo "$TS_URL_LINE"

echo "Usuario:     ${OPENCODE_SERVER_USERNAME}"
if [[ "$GENERATED" == "1" ]]; then
  echo "Clave:       ${OPENCODE_SERVER_PASSWORD}  (generada; para fijarla exportá OPENCODE_SERVER_PASSWORD)"
else
  echo "Clave:       (la de tu OPENCODE_SERVER_PASSWORD)"
fi
echo "Si la LAN no conecta: sudo ufw allow 4096/tcp comment 'opencode remote'"
echo ""

if [[ "$MODE" == "--web" ]]; then
  exec opencode web --port 4096 --hostname 0.0.0.0
else
  exec opencode serve --port 4096 --hostname 0.0.0.0
fi
