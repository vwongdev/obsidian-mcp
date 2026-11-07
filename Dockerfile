FROM rust:1.99-alpine AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --release --locked

FROM gcr.io/distroless/static-debian13:nonroot
COPY --from=build /src/target/release/obsidian-mcp /obsidian-mcp
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/obsidian-mcp"]
