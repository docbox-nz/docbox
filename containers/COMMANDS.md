
docker buildx build --platform linux/amd64,linux/arm64 -t jacobtread/docbox:0.1.0-alpine-prebuilt -t jacobtread/docbox:latest-alpine-prebuilt -f ./containers/prebuilt.alpine.Dockerfile --push . --build-arg GITHUB_RELEASE_VERSION=0.1.0


docker buildx build --platform linux/amd64,linux/arm64 -t jacobtread/docbox:0.1.0-bookworm-prebuilt -t jacobtread/docbox:latest-bookworm-prebuilt -t jacobtread/docbox:0.1.0 -t jacobtread/docbox:latest -f ./containers/prebuilt.bookworm.Dockerfile --push . --build-arg GITHUB_RELEASE_VERSION=0.1.0


