FROM node:22-trixie-slim AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev
COPY entrypoint.js ./

FROM gcr.io/distroless/nodejs22-debian13:nonroot
COPY --from=build /app /app
ENV VAULT_PATH=/vault XDG_CONFIG_HOME=/config
USER 65532:65532
CMD ["/app/entrypoint.js"]
