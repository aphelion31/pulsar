# Purpose: Reverse proxy in front of the Pulsar Axum server.
# Caller: Docker Compose.
# Dependencies: docker/nginx/nginx.conf.
# Main functions: Serve HTTP on :80 and proxy API, SPA assets, and /cable WebSockets to axum:3000.
# Side effects: Opens listener :80 inside the compose network.
FROM nginx:1.30.7-alpine

RUN addgroup -S circleit && adduser -S -u 1001 -G circleit circleit

COPY ./docker/nginx/nginx.conf /etc/nginx/nginx.conf

RUN touch /var/run/nginx.pid \
    && chown circleit:circleit /var/run/nginx.pid \
    && chown -R circleit:circleit /var/cache/nginx

USER circleit

EXPOSE 80

CMD ["nginx", "-g", "daemon off;"]
