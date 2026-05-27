import 'common.just'

# Install Git pre-commit hooks
[working-directory: repo]
setup-hooks:
    chmod +x .commit-hooks/pre-commit
    git config core.hooksPath .commit-hooks
    @echo "Pre-commit hooks installed."

# Clean build artifacts
[working-directory: repo]
clean:
    cargo clean
    cd frontends/web && rm -rf .next
