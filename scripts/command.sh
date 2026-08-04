curl -i -X PATCH http://localhost:8080/lobbies/test/admin \
  -H "Content-Type: application/json" \
  -d '
{
    "lobbyPassword": "password",
    "command": {
        "host": {
            "hostPassword": "host_password",
            "command": "getBuzzerQueue"
        }
    }
}  
'
