FROM oven/bun:1.3.11 AS deps
WORKDIR /app
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile --production

FROM oven/bun:1.3.11 AS build
WORKDIR /app
ENV EMDASH_DATA_DIR=/data/emdash
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile
COPY astro.config.mjs emdash-env.d.ts tsconfig.json ./
COPY seed ./seed
COPY src ./src
RUN bun run build

FROM node:24-bookworm-slim AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=3000
ENV EMDASH_DATA_DIR=/data/emdash
ENV EMDASH_AUTO_BOOTSTRAP=true
COPY --from=deps /app/node_modules ./node_modules
COPY --from=build /app/dist ./dist
COPY package.json bun.lock astro.config.mjs emdash-env.d.ts tsconfig.json ./
COPY seed ./seed
COPY docker-entrypoint.sh ./docker-entrypoint.sh
RUN chmod +x ./docker-entrypoint.sh && mkdir -p /data/emdash
EXPOSE 3000
CMD ["./docker-entrypoint.sh"]
