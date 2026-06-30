variable "REGISTRY_IMAGE" {
  default = "ghcr.io/bartdeijkers/ersatzrs"
}

variable "VERSION" {
  default = "dev"
}

group "default" {
  targets = ["ersatzrs"]
}

target "ersatzrs" {
  context = "."
  dockerfile = "Dockerfile"
  platforms = [
    "linux/amd64",
    "linux/arm64",
    "linux/arm/v7",
  ]
  tags = [
    "${REGISTRY_IMAGE}:${VERSION}",
  ]
}

target "ersatzrs-release" {
  inherits = ["ersatzrs"]
  dockerfile = "Dockerfile.release"
  target = "runtime"
}

target "docker-check" {
  inherits = ["ersatzrs"]
  platforms = ["linux/amd64"]
  tags = ["ersatzrs:ci"]
  output = ["type=docker"]
}
