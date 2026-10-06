#!/usr/bin/env bash
# certbot --deploy-hook for option A (docker compose + nginx): copies the new
# certificate into deploy/certs as real files (the container cannot follow
# certbot's symlinks into /etc/letsencrypt) and reloads nginx. certbot runs it
# after every successful issue or renewal with RENEWED_LINEAGE set.
#   sudo certbot certonly --webroot -w "$PWD/deploy/acme" -d geo.example.com \
#     --deploy-hook "$PWD/deploy/certbot-deploy-hook.sh"
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
lineage="${RENEWED_LINEAGE:?run by certbot --deploy-hook, which sets RENEWED_LINEAGE}"
install -d -m 0755 "$here/certs"
install -m 0644 "$(readlink -f "$lineage/fullchain.pem")" "$here/certs/fullchain.pem.new"
install -m 0600 "$(readlink -f "$lineage/privkey.pem")" "$here/certs/privkey.pem.new"
mv -f "$here/certs/fullchain.pem.new" "$here/certs/fullchain.pem"
mv -f "$here/certs/privkey.pem.new" "$here/certs/privkey.pem"
if [ -z "${GEOSOLVER_NO_RELOAD:-}" ]; then
  docker compose -f "$here/docker-compose.yml" exec -T nginx nginx -s reload
fi
echo "installed $(basename "$lineage") certificate into $here/certs"
