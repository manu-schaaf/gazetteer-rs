FROM rust:1.61.0

RUN rustup default nightly

WORKDIR /app
COPY src/ src/
COPY static/ static/
COPY templates/ templates/
COPY resources/ resources/
COPY *.toml .

RUN cargo build --features="server" --release

EXPOSE 8080
ENV ROCKET_ADRESS=0.0.0.0
ENV ROCKET_PORT=8080
#CMD ["cargo", "run", "--features", "server"]
CMD ["./target/release/gazetteer"]