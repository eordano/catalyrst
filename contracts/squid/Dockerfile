FROM node:24-alpine@sha256:5fa278c599dbba0c8f873d8717d50ecbb57c5ae6a53b7ab240c25135e0b65995 AS node
FROM node AS node-with-gyp
RUN apk add --no-cache g++ make python3

FROM node-with-gyp AS builder
WORKDIR /squid
ADD package.json .
ADD package-lock.json .
# remove if needed
ADD schema.graphql .
ADD squid.yaml .
RUN npm ci
ADD tsconfig.json .
ADD src src
ADD db db
ADD assets assets
RUN npm run build

FROM node-with-gyp AS deps
WORKDIR /squid
ADD package.json .
ADD package-lock.json .
RUN npm ci --production

FROM node AS squid
WORKDIR /squid

# Add build argument for commit hash
ARG COMMIT_HASH=local
ENV COMMIT_HASH=${COMMIT_HASH:-local}

COPY --from=deps /squid/package.json .
COPY --from=deps /squid/package-lock.json .
COPY --from=deps /squid/node_modules node_modules
COPY --from=builder /squid/lib lib
COPY --from=builder /squid/db db
COPY --from=builder /squid/assets assets
# remove if no schema.graphql is in the root
COPY --from=builder /squid/schema.graphql schema.graphql
COPY --from=builder /squid/squid.yaml squid.yaml
# remove if no commands.json is in the root
ADD commands.json .
# add indexer script
ADD indexer.sh /squid/indexer.sh
RUN chmod +x /squid/indexer.sh
RUN echo -e "loglevel=silent\\nupdate-notifier=false" > /squid/.npmrc
RUN npm i -g @subsquid/cli@latest && mv $(which sqd) /usr/local/bin/sqd

# Install jq and AWS CLI v1
# --break-system-packages is required since the node:24-alpine base image ships
# a PEP 668 "externally-managed" Python, which otherwise rejects the pip install.
RUN apk update && apk add --no-cache tini postgresql-client curl jq python3 py3-pip \
    && pip3 install --break-system-packages awscli \
    && rm -rf /var/cache/apk/*

ENV ETH_PROMETHEUS_PORT 3000
ENV POLYGON_PROMETHEUS_PORT 3001
ENV GQL_PORT 5000

RUN touch /squid/.env

# Entry point script
ADD entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
