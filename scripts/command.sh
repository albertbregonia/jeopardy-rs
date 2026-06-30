curl -i -X PATCH http://localhost:8080/lobbies/test/admin \
  -H "Content-Type: application/json" \
  -d '
{
    "lobby_password": "password",
    "command": {
        "Host": {
            "host_password": "host_password",
            "command": "GetBuzzerQueue"
        }
    }
}  
'
