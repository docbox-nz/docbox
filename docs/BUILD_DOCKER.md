# Building under docker

Build the builder docker container image:

```
docker build -t docbox-builder -f containers/BuildDocbox.Dockerfile .
```

Create a docker container for the builder:

```
docker create --name docbox-builder docbox-builder
```

Copy the built binary out of the builder container:

```
docker cp docbox-builder:/app/target/x86_64-unknown-linux-musl/release/docbox ./docbox
```

Remove the builder container:

```
docker rm docbox-builder
```