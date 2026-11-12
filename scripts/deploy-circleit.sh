#!/bin/sh
# Zero-downtime-ish redeploy of Pulsar on vps-circleit. Starts the new container first on a temp name,
# checks /health, then swaps. Old binary/web kept as *.prev for rollback.
set -eu
cd ~/pulsar
H=chatpulsargvfaypjnjlhrua.circleit.dev
NET=cit-chatwoot-postgres-network
run() { # $1 name, $2 bin dir suffix, $3 web dir, extra: traefik labels only for final name
  name=$1; bin=$2; web=$3; shift 3
  docker run -d --name "$name" --restart unless-stopped --cpus 2 --memory 512m --env-file .env --network $NET \
    -v "$HOME/pulsar/$bin:/app/pulsar:ro" -v "$HOME/pulsar/$web:/app/web:ro" -v "$HOME/pulsar/storage:/app/storage" \
    --log-opt max-size=10m --log-opt max-file=3 "$@" ubuntu:22.04 /app/pulsar >/dev/null
}
health() { docker run --rm --network $NET curlimages/curl:latest -sf -o /dev/null "http://$1:3000/health"; }

mkdir -p storage
rm -rf web.new && mkdir web.new && tar xzf web-dev.tgz -C web.new 2>/dev/null
chmod +x pulsar.new

docker rm -f pulsar-next >/dev/null 2>&1 || true
run pulsar-next pulsar.new web.new
ok=0; for i in $(seq 1 20); do if health pulsar-next; then ok=1; break; fi; sleep 1; done
if [ $ok != 1 ]; then echo "new build unhealthy, keeping current"; docker logs pulsar-next 2>&1 | tail -5; docker rm -f pulsar-next >/dev/null; exit 1; fi
docker rm -f pulsar-next >/dev/null

# swap files, then recreate the routed container
cp pulsar pulsar.prev; mv pulsar.new pulsar
rm -rf web.prev; mv web web.prev; mv web.new web
docker rm -f pulsar >/dev/null 2>&1 || true
run pulsar pulsar web \
  -l traefik.enable=true -l traefik.docker.network=traefik-network \
  -l "traefik.http.routers.pulsar-router.rule=Host(\`$H\`)" -l traefik.http.routers.pulsar-router.entryPoints=websecure \
  -l traefik.http.routers.pulsar-router.tls=true -l traefik.http.routers.pulsar-router.tls.certresolver=leResolver \
  -l traefik.http.routers.pulsar-router.service=pulsar-service -l traefik.http.services.pulsar-service.loadbalancer.server.port=3000
docker network connect traefik-network pulsar
for i in $(seq 1 20); do health pulsar && break; sleep 1; done
# Traefik needs a few seconds to pick up the recreated container; report only once the public URL answers.
for i in $(seq 1 30); do [ "$(curl -s -o /dev/null -w '%{http_code}' "https://$H/health")" = 200 ] && break; sleep 1; done
echo "public /health: $(curl -s -o /dev/null -w '%{http_code}' "https://$H/health")"
docker ps --filter name=^pulsar$ --format "{{.Names}} {{.Status}}"
