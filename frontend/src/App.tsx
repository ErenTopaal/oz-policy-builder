import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Nav } from "./sections/Nav";
import { Hero } from "./sections/Hero";
import { Problem } from "./sections/Problem";
import { HowItWorks } from "./sections/HowItWorks";
import { ProofPoint } from "./sections/ProofPoint";
import { PlaygroundShowcase } from "./sections/PlaygroundShowcase";
import { Primitives } from "./sections/Primitives";
import { Stats } from "./sections/Stats";
import { QuickStart } from "./sections/QuickStart";
import { Architecture } from "./sections/Architecture";
import { Faq } from "./sections/Faq";
import { Footer } from "./sections/Footer";
import { PlaygroundPage } from "./playground/PlaygroundPage";

function Landing() {
  return (
    <>
      <Nav />
      <Hero />
      <Problem />
      <HowItWorks />
      <ProofPoint />
      <PlaygroundShowcase />
      <Primitives />
      <Stats />
      <QuickStart />
      <Architecture />
      <Faq />
      <Footer />
    </>
  );
}

// router-less route table — exported for tests that supply their own
// router (e.g. MemoryRouter). production code uses <App/> which wraps
// this in a BrowserRouter.
export function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<Landing />} />
      <Route path="/playground" element={<PlaygroundPage />} />
      <Route path="/playground/s/:snapshotId" element={<PlaygroundPage />} />
    </Routes>
  );
}

export function App() {
  return (
    <BrowserRouter>
      <AppRoutes />
    </BrowserRouter>
  );
}
