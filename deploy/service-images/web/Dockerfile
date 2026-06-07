# syntax=docker/dockerfile:1.7

FROM oven/bun:1 AS deps
WORKDIR /workspace/Agentics

COPY package.json bun.lock ./
COPY frontends/web/package.json frontends/web/package.json
RUN bun install --frozen-lockfile

FROM oven/bun:1 AS builder
WORKDIR /workspace/Agentics

COPY --from=deps /workspace/Agentics/node_modules ./node_modules
COPY --from=deps /workspace/Agentics/frontends/web/node_modules ./frontends/web/node_modules
COPY frontends/web ./frontends/web
COPY skills/agentics-introduction ./skills/agentics-introduction
WORKDIR /workspace/Agentics/frontends/web
ENV AGENTICS_DEPLOYMENT_STAGE=production
ENV AGENTICS_API_BASE_URL=http://api:3100
ENV AGENTICS_WEB_PORT=3001
RUN bun run build

FROM oven/bun:1 AS runtime
WORKDIR /workspace/Agentics

ENV NODE_ENV=production
ENV AGENTICS_DEPLOYMENT_STAGE=production
ENV AGENTICS_API_BASE_URL=http://api:3100
ENV AGENTICS_WEB_PORT=3001
COPY --from=builder /workspace/Agentics ./
WORKDIR /workspace/Agentics/frontends/web

CMD ["bun", "run", "start", "--", "-H", "0.0.0.0", "-p", "3001"]
