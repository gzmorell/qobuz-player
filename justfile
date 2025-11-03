init-database:
    cargo sqlx db create && cargo sqlx migrate run --source qobuz-player-controls/migrations

[working-directory: 'qobuz-player-web']
build-styles:
    npm i
    npm run build

create-env-file:
    echo 'DATABASE_URL="sqlite:///tmp/qobuz-player.db"' > .env

build-all:
    just build-styles
    cargo build --release
