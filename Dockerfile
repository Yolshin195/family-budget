# syntax=docker/dockerfile:1
# Семейная бухгалтерия: сборка бинарника и минимальный runtime-образ.
# Работает с Podman/Buildah и Docker. Имена образов полные — Podman не спрашивает реестр.
#
# Зависимости (~380 крейтов) собираются отдельным слоем через cargo-chef: он пересобирается только при
# изменении Cargo.toml / Cargo.lock, а не при каждой правке кода. Это обычный слой, поэтому он попадает и в
# кэш Podman/Docker на машине, и в кэш GitHub Actions (type=gha) — в отличие от RUN --mount=type=cache,
# содержимое которого с CI-раннера никуда не сохраняется.

# ---------- инструменты сборки ----------
FROM docker.io/library/rust:1-slim-bookworm AS chef
# renovate: datasource=crate depName=cargo-chef
ARG CARGO_CHEF_VERSION=0.1.78
RUN cargo install cargo-chef --locked --version "${CARGO_CHEF_VERSION}"
WORKDIR /src

# ---------- рецепт зависимостей (только манифесты и Cargo.lock) ----------
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---------- сборка ----------
FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release -p family_budget --locked --recipe-path recipe.json
COPY . .
RUN cargo build --release -p family_budget --locked \
 && cp target/release/family_budget /usr/local/bin/family_budget

# ---------- запуск ----------
FROM docker.io/library/debian:bookworm-slim AS runtime
# upgrade — исправления безопасности, вышедшие после сборки базового образа (Trivy в CI падает на них).
# В CI эта стадия собирается без кэша слоёв (no-cache-filters: runtime), иначе upgrade взялся бы из старого кэша.
RUN apt-get update \
 && apt-get upgrade -y \
 && apt-get install -y --no-install-recommends ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home app
COPY --from=build /usr/local/bin/family_budget /usr/local/bin/family_budget
USER app
# Коммит сборки (CI передаёт github.sha) — версия в Sentry: family_budget@<версия>+<коммит>.
ARG GIT_SHA=
ENV BIND_ADDR=0.0.0.0:8080 \
    TZ=Asia/Bangkok \
    GIT_SHA=${GIT_SHA}
EXPOSE 8080
CMD ["family_budget"]
