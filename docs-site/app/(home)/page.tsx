import Link from 'next/link';

export const metadata = {
  title: 'oz-policy-builder — docs',
  description:
    'Records a Stellar transaction and synthesizes the smallest OpenZeppelin smart-account policy that permits exactly that transaction.',
  other: {
    'http-equiv': 'refresh',
  },
};

export default function HomePage() {
  return (
    <>
      <meta httpEquiv="refresh" content="0; url=/docs" />
      <div className="flex flex-col justify-center text-center flex-1 px-6 py-16">
        <h1 className="text-3xl font-semibold mb-3">oz-policy-builder docs</h1>
        <p className="text-fd-muted-foreground mb-6">
          Redirecting to the documentation home.
        </p>
        <p>
          If you are not redirected,{' '}
          <Link href="/docs" className="font-medium underline">
            open /docs
          </Link>
          .
        </p>
      </div>
    </>
  );
}
