import { en } from "./i18n/en";
import "./App.css";

export default function App() {
  return (
    <main className="app">
      <h1 className="app__name">{en.appName}</h1>
    </main>
  );
}
