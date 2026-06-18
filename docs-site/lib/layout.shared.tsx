import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { appName, gitConfig, landingUrl, playgroundUrl } from './shared';

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: appName,
    },
    githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
    links: [
      {
        text: 'Playground',
        url: playgroundUrl,
        external: true,
      },
      {
        text: 'Landing',
        url: landingUrl,
        external: true,
      },
    ],
  };
}
