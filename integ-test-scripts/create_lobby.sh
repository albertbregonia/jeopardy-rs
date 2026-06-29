curl -i -X POST http://localhost:8080/lobbies \
  -H "Content-Type: application/json" \
  -d '
{
    "lobby_name": "test",
    "lobby_password": "password",
    "host_password": "host_password",
    "config": {
        "boards": [
            {
                "categories": [
                    {
                        "name": "test",
                        "questions": [
                            {
                                "point_value": 0,
                                "daily_double": false,
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
        "final_jeopardy": {
            "hint": "hint",
            "question": {
                "content": "final_jeopardy",
                "answer": "answer"
            }
        }
    }
}  
'
