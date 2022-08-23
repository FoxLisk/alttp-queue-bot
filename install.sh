set -e
SERVICE_NAME="alttp-queue-bot"
SERVICE_PATH="/lib/systemd/system/$SERVICE_NAME.service"
NGINX_TARGET_PATH="/etc/nginx/conf.d/$SERVICE_NAME.conf"
cargo build
sudo cp "conf_files/lib/systemd/system/$SERVICE_NAME.service" $SERVICE_PATH
sudo systemctl daemon-reload
sudo systemctl restart "$SERVICE_NAME"
sudo systemctl enable "$SERVICE_NAME"
