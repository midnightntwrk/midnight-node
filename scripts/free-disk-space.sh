#!/bin/bash
set -euo pipefail

# Free disk space on GitHub action runners
# Script inspired by https://github.com/rust-lang/rust/blob/master/src/ci/scripts/free-disk-space.sh
# and https://github.com/jlumbroso/free-disk-space

isX86() {
    local arch
    arch=$(uname -m)
    if [ "$arch" = "x86_64" ]; then
        return 0
    else
        return 1
    fi
}

# Check if we're on a GitHub hosted runner.
# This is critical - we don't want to run this on developer machines!
isGitHubRunner() {
    # `:-` means "use the value of RUNNER_ENVIRONMENT if it exists, otherwise use an empty string".
    if [[ "${RUNNER_ENVIRONMENT:-}" == "github-hosted" ]]; then
        return 0
    else
        return 1
    fi
}

# Safety check: Only run on GitHub runners
if ! isGitHubRunner; then
    echo "::error::This script is only for GitHub-hosted runners. Refusing to run on local machine."
    echo "RUNNER_ENVIRONMENT=${RUNNER_ENVIRONMENT:-not set}"
    exit 1
fi

removeUnusedFilesAndDirs() {
    local to_remove=(
        # Android
        "/usr/local/lib/android"

        # Haskell
        "/opt/ghc"
        "/usr/local/.ghcup"

        # .NET
        "/usr/share/dotnet"

        # Hosted toolcache (includes various versions of Node, Python, Ruby, etc.)
        "/opt/hostedtoolcache"

        # Java
        "/usr/lib/jvm"
        "/usr/share/java"

        # AWS tools
        "/usr/local/aws-sam-cli"

        # Build tools
        "/usr/local/doc/cmake"
        "/usr/local/share/cmake-"*

        # Julia
        "/usr/local/julia"*

        # Browsers and drivers
        "/usr/local/share/chromedriver-"*
        "/usr/local/share/chromium"
        "/usr/local/share/edge_driver"
        "/usr/local/share/gecko_driver"

        # Editors
        "/usr/local/share/emacs"
        "/usr/local/share/vim"

        # Other dev tools
        "/usr/local/share/powershell"
        "/usr/local/share/vcpkg"
        "/usr/local/share/icons"

        # Additional libraries
        "/usr/local/lib/node_modules"
        "/usr/local/lib/heroku"

        # Azure CLI
        "/opt/az"
        "/opt/microsoft"
        "/opt/google"

        # Maven, Gradle, Kotlin
        "/usr/share/apache-maven-"*
        "/usr/share/gradle-"*
        "/usr/share/kotlinc"

        # Python/Conda
        "/usr/share/miniconda"

        # PHP, Ruby, Swift
        "/usr/share/php"
        "/usr/share/ri"
        "/usr/share/swift"

        # Binaries we don't need
        "/usr/local/bin/azcopy"
        "/usr/local/bin/bicep"
        "/usr/local/bin/ccmake"
        "/usr/local/bin/cmake-"*
        "/usr/local/bin/cmake"
        "/usr/local/bin/cpack"
        "/usr/local/bin/ctest"
        "/usr/local/bin/kind"
        "/usr/local/bin/minikube"
        "/usr/local/bin/packer"
        "/usr/local/bin/phpunit"
        "/usr/local/bin/pulumi-"*
        "/usr/local/bin/pulumi"
        "/usr/local/bin/stack"
    )

    if [ -n "${AGENT_TOOLSDIRECTORY:-}" ]; then
        # Environment variable set by GitHub Actions
        to_remove+=(
            "${AGENT_TOOLSDIRECTORY}"
        )
    else
        echo "::warning::AGENT_TOOLSDIRECTORY is not set. Skipping removal."
    fi

    # Remove all files and directories at once to save time.
    sudo rm -rf "${to_remove[@]}"
}

# Remove large packages
# REF: https://github.com/apache/flink/blob/master/tools/azure-pipelines/free_disk_space.sh
cleanPackages() {
    local packages=(
        '^aspnetcore-.*'
        '^dotnet-.*'
        '^llvm-.*'
        '^mongodb-.*'
        'azure-cli'
        'firefox'
        'libgl1-mesa-dri'
        'microsoft-edge-stable'
        'mono-devel'
        'php.*'
    )

    if isX86; then
        packages+=(
            'google-chrome-stable'
            'google-cloud-cli'
            'google-cloud-sdk'
            'powershell'
        )
    fi

    WAIT_DPKG_LOCK="-o DPkg::Lock::Timeout=60"
    sudo apt-get "${WAIT_DPKG_LOCK}" -qq remove -y --fix-missing "${packages[@]}" \
        || echo "::warning::Some packages failed to remove"

    sudo apt-get "${WAIT_DPKG_LOCK}" autoremove -y \
        || echo "::warning::The command [sudo apt-get autoremove -y] failed"
    sudo apt-get "${WAIT_DPKG_LOCK}" clean \
        || echo "::warning::The command [sudo apt-get clean] failed"
}

# Remove Docker images, containers, volumes, and build cache.
# Ubuntu 22 runners have docker images already installed.
cleanDocker() {
    echo "=> Removing the following docker images:"
    sudo docker image ls
    echo "=> Removing docker images, containers, volumes, and build cache..."
    sudo docker system prune -af || true
    sudo docker builder prune -af || true
}

# Remove Swap storage
cleanSwap() {
    sudo swapoff -a || true
    sudo rm -rf /mnt/swapfile || true
    free -h
}

# Display initial disk space stats
echo "Initial disk space:"
df -h /
# cleanPackages - slow and doesn't free much.
cleanDocker
cleanSwap
removeUnusedFilesAndDirs
echo "Final disk space:"
df -h /
