sudo docker ps -a

sudo docker stop scylla-node1
sudo docker rm scylla-node1

sudo docker compose -f docker-compose.yml up -d --wait