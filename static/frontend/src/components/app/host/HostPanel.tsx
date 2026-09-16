import "./HostPanel.css";

interface HostPanelProps {

}

export function HostPanel({ }: HostPanelProps) {
    return (
        <div className="host-panel">
            <form className="host-panel-form" onSubmit={e => e.preventDefault()}>
                <label>Buzzer Queue</label>
                {/* 
                    TODO: 'Show Queue' will start the dequeue process (live polling for now)
                    'Correct' and 'Incorrect' will add or deduct points from the current player
                    and will clear queue or dequeue if correct or incorrect respectively
                    'Show Answer' will clear the queue and broadcast the answer (stop polling as well)
                */}
                <button>Correct</button>
                <button>Incorrect</button>
                <button>Show Answer</button>
                <button>Show Queue</button>
            </form>
            <form className="host-panel-form" onSubmit={e => e.preventDefault()}>
                <label>Final Jeopardy</label>
                <button>Show Hint</button>
                <button>Show Question</button>
                <button>Show Answer</button>
            </form>
            <form className="host-panel-form" onSubmit={e => e.preventDefault()}>
                <label>Player Commands</label>
                <input type="text" placeholder="Player Name" />
                <input type="text" placeholder="Points" />
                <button>Set Points</button>
                <button>Update Points</button>
            </form>
        </div>
    )
}