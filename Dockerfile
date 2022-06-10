FROM rust:1.61.0

RUN rustup default nightly

WORKDIR /app
COPY src/ src/
COPY static/ static/
COPY templates/ templates/
COPY resources/ resources/
COPY *.toml .

RUN cargo build --features="server" --release

EXPOSE 80
CMD ["./target/release/gazetteer"]