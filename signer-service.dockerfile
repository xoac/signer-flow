####################################################################################################
## Builder
####################################################################################################
FROM rust:latest AS builder

RUN rustup target add x86_64-unknown-linux-musl
RUN apt update && apt install -y musl-tools musl-dev \
  cmake \
  build-essential \
  curl \
  openssl libssl-dev \
  pkg-config \
  python \
  valgrind \
  zlib1g-dev
RUN update-ca-certificates
RUN ln -s "/usr/bin/g++" "/usr/bin/musl-g++"

# Create appuser
ENV USER=appuser
ENV UID=10001

RUN adduser \
  --disabled-password \
  --gecos "" \
  --home "/nonexistent" \
  --shell "/sbin/nologin" \
  --no-create-home \
  --uid "${UID}" \
  "${USER}"


WORKDIR /signer-service

# cache dependencies
RUN cargo init
COPY ./signer-rest-api/Cargo.toml ./
RUN cargo build --target x86_64-unknown-linux-musl --release

COPY ./ .
RUN cargo build -p signer-service --target x86_64-unknown-linux-musl --release

####################################################################################################
## Final image
####################################################################################################
FROM scratch

# Import from builder.
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

WORKDIR /signer-service

# Copy our build
COPY --from=builder /signer-service/target/x86_64-unknown-linux-musl/release/signer-service ./

# Use an unprivileged user.
USER appuser:appuser

CMD ["/signer-service/signer-service"]
