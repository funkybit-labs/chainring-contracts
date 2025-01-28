#!/bin/bash

echo -e "\nRunning start containers"

docker compose -p arch-bitcoin-network down --remove-orphans
docker compose -p arch-bitcoin-network up -d
sleep 8
docker compose -f docker-compose-arch.yaml up -d

echo "Patching mempool-frontend Nginx config"
# We are patching nginx config of mempool-frontend container to mimic production deployment of mempool
# in order to forward certain API calls to electrs
#
# replaces
#
# location /api/ {
#   proxy_pass http://mempool-backend:8999/api/v1/;
# }
#
# with
#
# location /api/ {
#   proxy_intercept_errors on;
#   error_page 404 = @esplora;
#   proxy_pass http://mempool-backend:8999/api/v1/;
# }
#
# location @esplora {
#   rewrite_log on;
#   rewrite ^/api/(.*) /\$1 break;
#   proxy_pass http://electrs:3001;
# }
docker exec --user root mempool-frontend sed -i 's,proxy_pass http://mempool-backend:8999/api/v1/;,proxy_intercept_errors on; error_page 404 = @esplora; proxy_pass http://mempool-backend:8999/api/v1/;,g' /etc/nginx/conf.d/nginx-mempool.conf
docker exec --user root mempool-frontend sed -i 's,proxy_pass http://mempool-backend:8999/api/v1;,proxy_intercept_errors on; error_page 404 = @esplora; proxy_pass http://mempool-backend:8999/api/v1/;,g' /etc/nginx/conf.d/nginx-mempool.conf
docker exec --user root mempool-frontend sh -c "echo 'location @esplora {  rewrite_log on; rewrite ^/api/(.*) /\$1 break; proxy_pass http://electrs:3001; }' >> /etc/nginx/conf.d/nginx-mempool.conf"

echo "Done!"
