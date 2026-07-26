import { Category } from "./components/category/Category"
import type { Category as JeopardyCategory } from "./types/Jeopardy";
import './App.css'

function App() {
    // dummy data to fill the components
    const category: JeopardyCategory = {
        name: "Category Name",
        questions: Array.from({ length: 6 }, () => ({
            answered: false,
            dailyDouble: false,
            pointValue: Math.round(Math.random() * 1000 + 100),
            question: {
                content: "Question",
                answer: "Answer"
            }
        }))
    };
    const longNamedCategory: JeopardyCategory = {
        name: "Lorem ipsum dolor sit amet consectetur, adipisicing elit.Eos esse doloremque eum corrupti nam quibusdam perspiciatis iste veniam molestias, aut sed pariatur numquam totam, vel similique quod mollitia a.Velit!",
        questions: Array.from({ length: 1 }, () => ({
            answered: false,
            dailyDouble: false,
            pointValue: Math.round(Math.random() * 1000 + 100),
            question: {
                content: "Question",
                answer: "Answer"
            }
        }))
    };
    return (
        <div className="test">
            <Category category={longNamedCategory} />
            <Category category={category} />
            <Category category={category} />
            <Category category={category} />
            <Category category={category} />
            <Category category={category} />
        </div>
    )
}

export default App
