#!/bin/sh
# Copies the site to the web server over ssh: index.html, web.config and media/.
#   sh website/deploy.sh
#   WEB_HOST=… WEB_DIR=… sh website/deploy.sh   (or put both in website/deploy.env)
# The server side is a non-admin "deploy" account that can write only the site folder (see README.md).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
# The server's name and folder live in deploy.env beside this script (not in git): WEB_HOST=deploy@… and WEB_DIR=C:/….
[ -f "$here/deploy.env" ] && . "$here/deploy.env"
host=${WEB_HOST:?set WEB_HOST (deploy@server) in website/deploy.env}
dir=${WEB_DIR:-C:/inetpub/wwwroot/jackharvest.com/shapecraft}
key=${WEB_KEY:-$HOME/.ssh/id_ed25519_web}
# Media first, the page last, so a visitor mid-deploy never gets a page pointing at missing files.
scp -i "$key" -o IdentitiesOnly=yes -o BatchMode=yes -r "$here/media" "$host:/$dir/"
scp -i "$key" -o IdentitiesOnly=yes -o BatchMode=yes "$here/web.config" "$here/index.html" "$host:/$dir/"
echo "deployed to $host:$dir"
