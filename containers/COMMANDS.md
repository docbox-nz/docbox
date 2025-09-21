
docker buildx build --platform linux/amd64,linux/arm64 -t jacobtread/docbox:0.3.2-alpine-prebuilt -t jacobtread/docbox:latest-alpine-prebuilt -f ./containers/prebuilt.alpine.Dockerfile --push . --build-arg GITHUB_RELEASE_VERSION=0.3.2


docker buildx build --platform linux/amd64,linux/arm64 -t jacobtread/docbox:0.3.2-bookworm-prebuilt -t jacobtread/docbox:latest-bookworm-prebuilt -t jacobtread/docbox:0.3.2 -t jacobtread/docbox:latest -f ./containers/prebuilt.bookworm.Dockerfile --push . --build-arg GITHUB_RELEASE_VERSION=0.3.2


