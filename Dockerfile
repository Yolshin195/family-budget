# syntax=docker/dockerfile:1
# Семейная бухгалтерия: сборка бинарника и минимальный runtime-образ.
# Работает с Podman/Buildah и Docker. Имена образов полные — Podman не спрашивает реестр.

# ---------- сборка ----------
FROM docker.io/library/rust:1-slim-bookworm AS build
WORKDIR /src
# Кэш реестра crates.io и target между сборками (BuildKit / Buildah ≥ 1.24).
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p family_budget --locked \
 && cp target/release/family_budget /usr/local/bin/family_budget

# ---------- запуск ----------
FROM docker.io/library/debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home app
COPY --from=build /usr/local/bin/family_budget /usr/local/bin/family_budget
USER app
ENV BIND_ADDR=0.0.0.0:8080 \
    TZ=Asia/Bangkok
EXPOSE 8080
CMD ["family_budget"]
