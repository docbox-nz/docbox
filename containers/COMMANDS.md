
docker buildx build --platform linux/amd64 -t jacobtread/docbox:0.1.0-bookworm-prebuilt-amd64 -f ./containers/prebuilt.bookworm.amd64.Dockerfile --push .
docker buildx build --platform linux/amd64 -t jacobtread/docbox:0.1.0-alpine-prebuilt-amd64 -f ./containers/prebuilt.alpine.amd64.Dockerfile --push .


docker buildx build --platform linux/arm64 -t jacobtread/docbox:0.1.0-bookworm-prebuilt-arm64 -f ./containers/prebuilt.bookworm.arm64.Dockerfile --push .
docker buildx build --platform linux/arm64 -t jacobtread/docbox:0.1.0-alpine-prebuilt-arm64 -f ./containers/prebuilt.alpine.arm64.Dockerfile --push .