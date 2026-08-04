curl -i -X POST http://localhost:8080/lobbies \
  -H "Content-Type: application/json" \
  -d '
{
    "lobbyName": "test",
    "lobbyPassword": "password",
    "hostPassword": "host_password",
    "config": {
        "boards": [
            {
                "categories": [
                    {
                        "name": "test",
                        "questions": [
                            {
                                "pointValue": 0,
                                "dailyDouble": false,
                                "answered": false,
                                "question": {
                                    "content": "test_content",
                                    "answer": "test_answer"
                                }
                            }
                        ]
                    }
                ]
            }
        ],
        "finalJeopardy": {
            "hint": "hint",
            "question": {
                "content": "final_jeopardy",
                "answer": "answer"
            }
        }
    }
}  
'
