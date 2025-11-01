FROM docker.io/joseluisq/rust-linux-darwin-builder:2.0.0-beta.1-amd64 as build
COPY . .
RUN cargo build --release --target aarch64-apple-darwin
# RUN cargo build --release --target x86_64-apple-darwin

# isolate the binary
FROM scratch
COPY --from=build /root/src/target/aarch64-apple-darwin/release/sidecar /
ENTRYPOINT ["/sidecar"]
