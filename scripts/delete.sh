curl -i -X DELETE http://localhost:8080/lobbies/test \
  -H "Content-Type: application/json" \
  -d '{
    "force": true,
    "lobby_password": "password",
    "host_password": "host_password"
}'