VERSION 0.6
FROM purtontech/rust-on-nails-devcontainer:1.0.17

# Proto compiler and web grpc.
RUN sudo apt update \
    && sudo apt install -y protobuf-compiler \
    && sudo curl -OL https://github.com/grpc/grpc-web/releases/download/1.3.0/protoc-gen-grpc-web-1.3.0-linux-x86_64  \
    && sudo mv protoc-gen-grpc-web* /usr/local/bin/protoc-gen-grpc-web \
    && sudo chmod +x /usr/local/bin/protoc-gen-grpc-web

ARG APP_EXE_NAME=cloak
ARG CLI_EXE_NAME=cli
ARG CLI_LINUX_EXE_NAME=cloak-linux
ARG CLI_MACOS_EXE_NAME=cloak-macos
ARG DBMATE_VERSION=1.15.0

# Folders
ARG AXUM_FOLDER=crates/axum-server
ARG DB_FOLDER=crates/db
ARG GRPC_API_FOLDER=crates/grpc-api
ARG PIPELINE_FOLDER=crates/asset-pipeline

# Base images
ARG ENVOY_PROXY=envoyproxy/envoy:v1.17-latest
ARG NGINX=nginx:1.21.5
ARG KUBECTL=bitnami/kubectl:latest

# This file builds the following containers
ARG APP_IMAGE_NAME=purtontech/cloak-server:latest
ARG MIGRATIONS_IMAGE_NAME=purtontech/cloak-db-migrations:latest
ARG ENVOY_IMAGE_NAME=purtontech/cloak-envoy:latest
ARG WWW_IMAGE_NAME=purtontech/cloak-website:latest
ARG KUBERNETES_NAME=purtontech/cloak-kubernetes:latest


WORKDIR /build

USER vscode

dev:
    BUILD +pull-request
    # On github this check is performed directly by the action
    BUILD +check-selenium-failure

pull-request:
    BUILD +migration-container
    BUILD +app-container
    BUILD +envoy-container
    BUILD +integration-test

all:
    BUILD +migration-container
    BUILD +app-container
    BUILD +envoy-container
    BUILD +build-cli-osx
    BUILD +kubernetes-container
    BUILD +save-artifacts

npm-deps:
    COPY $PIPELINE_FOLDER/package.json $PIPELINE_FOLDER/package.json
    COPY $PIPELINE_FOLDER/package-lock.json $PIPELINE_FOLDER/package-lock.json
    RUN cd $PIPELINE_FOLDER && npm install
    SAVE ARTIFACT $PIPELINE_FOLDER/node_modules

npm-build:
    FROM +npm-deps
    COPY $PIPELINE_FOLDER $PIPELINE_FOLDER
    COPY --if-exists $GRPC_API_FOLDER $GRPC_API_FOLDER
    COPY +npm-deps/node_modules $PIPELINE_FOLDER/node_modules
    RUN cd $PIPELINE_FOLDER && npm run release
    SAVE ARTIFACT $PIPELINE_FOLDER/dist

prepare-cache:
    # Copy in all our crates
    COPY --dir crates crates
    COPY Cargo.lock Cargo.toml .
    RUN cargo chef prepare --recipe-path recipe.json --bin $AXUM_FOLDER
    SAVE ARTIFACT recipe.json

build-cache:
    COPY +prepare-cache/recipe.json ./
    RUN cargo chef cook --release --target x86_64-unknown-linux-musl
    SAVE ARTIFACT target
    SAVE ARTIFACT $CARGO_HOME cargo_home
    SAVE IMAGE --cache-hint

build:
    # Copy in all our crates
    COPY --dir crates crates
    COPY --dir Cargo.lock Cargo.toml .
    COPY +build-cache/cargo_home $CARGO_HOME
    COPY +build-cache/target target
    COPY --dir +npm-build/dist $PIPELINE_FOLDER/
    # We need to run inside docker as we need postgres running for cornucopia
    ARG DATABASE_URL=postgresql://postgres:testpassword@localhost:5432/postgres?sslmode=disable
    USER root
    WITH DOCKER \
        --pull postgres:alpine
        RUN docker run -d --rm --network=host -e POSTGRES_PASSWORD=testpassword postgres:alpine \
            && while ! pg_isready --host=localhost --port=5432 --username=postgres; do sleep 1; done ;\
                dbmate --migrations-dir $DB_FOLDER/migrations up \
            && cargo build --release --target x86_64-unknown-linux-musl
    END
    SAVE ARTIFACT target/x86_64-unknown-linux-musl/release/$APP_EXE_NAME
    SAVE ARTIFACT target/x86_64-unknown-linux-musl/release/$CLI_EXE_NAME

save-artifacts:
    FROM +build
    SAVE ARTIFACT target/x86_64-unknown-linux-musl/release/$APP_EXE_NAME AS LOCAL ./tmp/app
    SAVE ARTIFACT target/x86_64-unknown-linux-musl/release/$CLI_EXE_NAME AS LOCAL ./tmp/$CLI_LINUX_EXE_NAME

migration-container:
    FROM debian:bullseye-slim
    RUN apt-get update -y \  
        && apt-get install -y --no-install-recommends ca-certificates curl libpq-dev \
        && rm -rf /var/lib/apt/lists/*
    RUN curl -OL https://github.com/amacneil/dbmate/releases/download/v$DBMATE_VERSION/dbmate-linux-amd64 \
        && mv ./dbmate-linux-amd64 /usr/bin/dbmate \
        && chmod +x /usr/bin/dbmate
    COPY --dir $DB_FOLDER .
    CMD dbmate up
    SAVE IMAGE --push $MIGRATIONS_IMAGE_NAME

# To test this locally run
# docker run -it --rm -e APP_DATABASE_URL=$APP_DATABASE_URL -p 7403:7403 purtontech/trace-server:latest
app-container:
    FROM scratch
    COPY +build/$APP_EXE_NAME axum-server
    # Place assets in a build folder as that's where statics is expecting them.
    COPY --dir +npm-build/dist /build/$PIPELINE_FOLDER/
    COPY --dir $PIPELINE_FOLDER/images /build/$PIPELINE_FOLDER/images
    ENTRYPOINT ["./axum-server"]
    SAVE IMAGE --push $APP_IMAGE_NAME

envoy-container:
    FROM $ENVOY_PROXY
    COPY .devcontainer/envoy.yaml /etc/envoy/envoy.yaml
    # Update the first entry in our config to point at the marketing pages
    RUN sed -i '0,/development/{s/development/www/}' /etc/envoy/envoy.yaml
    RUN sed -i '0,/7104/{s/7104/80/}' /etc/envoy/envoy.yaml
    # The second development entry in our cluster list is the app
    RUN sed -i '0,/development/{s/development/app/}' /etc/envoy/envoy.yaml
    SAVE IMAGE $ENVOY_IMAGE_NAME

integration-test:
    FROM +build
    COPY .devcontainer/docker-compose.yml ./ 
    COPY .devcontainer/docker-compose.earthly.yml ./ 
    ARG DATABASE_URL=postgresql://postgres:testpassword@localhost:5432/cloak?sslmode=disable
    ARG APP_DATABASE_URL=postgresql://application:testpassword@db:5432/cloak
    # We expose selenium to localhost
    ARG WEB_DRIVER_URL='http://localhost:4444' 
    # The selenium container will connect to the envoy container
    ARG WEB_DRIVER_DESTINATION_HOST='http://envoy:7100' 
    # How do we connect to mailhog
    ARG MAILHOG_URL=http://localhost:8025/api/v2/messages?limit=1
    USER root
    WITH DOCKER \
        --compose docker-compose.yml \
        --compose docker-compose.earthly.yml \
        --service db \
        --service auth \
        --service smtp \
        # Record our selenium session
        --service selenium \
        --pull selenium/video:ffmpeg-4.3.1-20220208 \
        # Bring up the containers we have built
        --load $APP_IMAGE_NAME=+app-container \
        --load $ENVOY_IMAGE_NAME=+envoy-container

        # Force to command to always be succesful so the artifact is saved. 
        # https://github.com/earthly/earthly/issues/988
        RUN dbmate --migrations-dir $DB_FOLDER/migrations up \
            && docker run -d -p 7103:7103 --rm --network=build_default \
                -e APP_DATABASE_URL=$APP_DATABASE_URL \
                -e INVITE_DOMAIN=http://envoy:7100 \
                -e INVITE_FROM_EMAIL_ADDRESS=support@cloak.com \
                -e SMTP_HOST=smtp \
                -e SMTP_PORT=1025 \
                -e SMTP_USERNAME=thisisnotused \
                -e SMTP_PASSWORD=thisisnotused \
                -e SMTP_TLS_OFF='true' \
                --name app $APP_IMAGE_NAME \
            && docker run -d -p 7100:7100 -p 7101:7101 --rm --network=build_default --name envoy $ENVOY_IMAGE_NAME \
            && cargo test --no-run --release --target x86_64-unknown-linux-musl \
            && docker run -d --name video --network=build_default -e DISPLAY_CONTAINER_NAME=build_selenium_1 -e FILE_NAME=chrome-video.mp4 -v /build/tmp:/videos selenium/video:ffmpeg-4.3.1-20220208 \
            && (cargo test --release --target x86_64-unknown-linux-musl -- --nocapture || echo fail > ./tmp/fail) \
            && docker stop app envoy video
    END
    # You need the tmp/* if you use just tmp earthly will overwrite the folder
    SAVE ARTIFACT tmp/* AS LOCAL ./tmp/earthly/

check-selenium-failure:
    FROM +integration-test
    # https://github.com/earthly/earthly/issues/988
    # If we failed in selenium a fail file will have been created
    # to get build to pass and see video, run +pull-request
    IF [ -f ./tmp/earthly/fail ]
        RUN echo "cargo test has failed." && exit 1
    END

build-cli-osx:
    FROM joseluisq/rust-linux-darwin-builder:1.62.1
    COPY --dir Cargo.lock Cargo.toml crates .
    RUN apt-get update \
        && apt-get install -y --no-install-recommends \
            protobuf-compiler \
        #
        #
        # Clean up
        && apt-get autoremove -y \
        && apt-get clean -y \
        && rm -r /var/cache/* /var/lib/apt/lists/*

    RUN cd crates/cli \ 
        && CC=o64-clang \
        CXX=o64-clang++ \
        cargo build --release --target x86_64-apple-darwin
    SAVE ARTIFACT target/x86_64-apple-darwin/release/$CLI_EXE_NAME AS LOCAL ./tmp/$CLI_MACOS_EXE_NAME

kubernetes-container:
    FROM debian:11-slim
    COPY +build/$CLI_EXE_NAME /usr/local/bin/cloak
    RUN apt-get update \
        && apt-get install -y --no-install-recommends \
            ca-certificates \
            curl \
            wget
    RUN curl -LO https://storage.googleapis.com/kubernetes-release/release/$(curl -s https://storage.googleapis.com/kubernetes-release/release/stable.txt)/bin/linux/amd64/kubectl
    RUN chmod +x ./kubectl
    RUN mv ./kubectl /usr/local/bin
    CMD cloak --ecdh-private-key-file /cloak/cloak.pem env > tmp.env && kubectl create secret generic \$NAME --dry-run=client -o yaml --from-env-file tmp.env | kubectl apply -f -
    SAVE IMAGE $KUBERNETES_NAME
