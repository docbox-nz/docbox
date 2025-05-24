
## Requirements (For building and deploying)

- Rust & Cargo (Install through rustup) https://www.rust-lang.org/learn/get-started
- Git (https://git-scm.com/downloads)
- AWS CLI (https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html)
- Terraform https://developer.hashicorp.com/terraform/install?product_intent=terraform

On windows you also require:

- Cross https://github.com/cross-rs/cross

to cross compile for linux (The easy way) you can install this after installing rust 

```
cargo install cross --git https://github.com/cross-rs/cross
```

## Additional requirements (Linux desktop only)

We do builds for [MUSL](https://musl.libc.org/) instead of linking to glibc as the binaries are far more portable so theres no chance of having the wrong glibc version on a deployed server. Basically makes it easy all round for building locally and deploying, saves us the time it would take to 
try and build on the server

When building on linux you will need to install some libraries:

**On Fedora**:
```sh
sudo dnf install musl-gcc
```

**On Ubuntu/Debian**

```sh
sudo apt install -y musl-tools musl-dev
```

## Requirements (Local development only)

- Docker & Docker Compose (Docker compose should come bundled)
- [Poppler](https://poppler.freedesktop.org/)

You need to install Poppler if you are running the server directly on your computer. If you are running the server using the [Dockerfile](./containers/Docbox.Dockerfile) it will be installed within the docker image 


## Terraform (Infrastructure & Deploying)

For setting up the infrastructure and deploying to it you can find the terraform instructions [Here](./docs/TERRAFORM.md)

## Local testing 

To run the server in a local mocking mode, start up the docker compose 

```sh
docker compose up
```

Then run the server with the `mock-browser` flag

```sh
cargo run --features mock-browser -p docbox
```

This will mock the tenant and user headers and also enables CORS, this will allow you to replace the URL
in the frontend code and have it directly access the backend instead of going through the official API

## Building (Windows)

Install cross for compiling for linux:

https://github.com/cross-rs/cross

1. Install rust you'll get cargo with that 
2. Run `cargo install cross --git https://github.com/cross-rs/cross` this will install cross

Ensure you have docker installed and docker desktop running

Then use the following command to build: 

```sh
cross build -p docbox --target x86_64-unknown-linux-musl --release
```

## Building (Linux)

```sh
cargo build -p docbox --target x86_64-unknown-linux-musl --release
```