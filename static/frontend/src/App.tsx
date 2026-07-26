import { Cell } from "./components/cell/Cell"

function App() {
    return (
        <>
            <div style={{ display: "flex", flexDirection: "row" }}>
                <Cell>Sample Cell 1</Cell>
                <Cell>Sample Cell 2</Cell>
                <Cell>Sample Cell 3</Cell>
                <Cell>Sample Cell 4</Cell>
            </div>
        </>
    )
}

export default App
