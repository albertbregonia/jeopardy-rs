curl -i -X DELETE http://localhost:8080/lobbies/test \
  -H "Content-Type: application/json" \
  -d '{
    "force": true,
    "lobbyPassword": "password",
    "hostPassword": "host_password"
}'