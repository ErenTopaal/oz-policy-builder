import { Nav } from "./sections/Nav";
import { Hero } from "./sections/Hero";
import { Problem } from "./sections/Problem";
import { HowItWorks } from "./sections/HowItWorks";
import { ProofPoint } from "./sections/ProofPoint";
import { Synthesizer } from "./sections/Synthesizer";
import { Primitives } from "./sections/Primitives";
import { Stats } from "./sections/Stats";
import { QuickStart } from "./sections/QuickStart";
import { Architecture } from "./sections/Architecture";
import { Faq } from "./sections/Faq";
import { Footer } from "./sections/Footer";

export function App() {
  return (
    <>
      <Nav />
      <Hero />
      <Problem />
      <HowItWorks />
      <ProofPoint />
      <Synthesizer />
      <Primitives />
      <Stats />
      <QuickStart />
      <Architecture />
      <Faq />
      <Footer />
    </>
  );
}
