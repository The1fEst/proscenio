import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import CodeBlock from '@theme/CodeBlock';
import Layout from '@theme/Layout';
import type {ReactNode} from 'react';

import styles from './index.module.css';

type Feature = {
  title: string;
  body: ReactNode;
  to: string;
};

const features: Feature[] = [
  {
    title: 'A bar on every screen',
    body: (
      <>
        Workspaces with their app icons, the playing media, system resources, the clock, the tray,
        the battery and the weather.
      </>
    ),
    to: '/docs/bar',
  },
  {
    title: 'A sidebar of quick toggles',
    body: (
      <>
        Brightness and volume sliders, a grid of toggles you can rearrange, and the notification
        list.
      </>
    ),
    to: '/docs/sidebar',
  },
  {
    title: 'Its own notification server',
    body: (
      <>
        Popups grouped by app, their actions, a history kept across restarts, and an unread count
        in the bar.
      </>
    ),
    to: '/docs/notifications',
  },
  {
    title: 'Launcher and overview',
    body: (
      <>
        Search apps, run math, reach the clipboard and your own actions, over a grid of workspaces
        with live window previews.
      </>
    ),
    to: '/docs/overview',
  },
  {
    title: 'A settings app',
    body: (
      <>
        Network, Bluetooth, displays, sound, power, input and the look of the shell and of Hyprland,
        written straight into Hyprland's config.
      </>
    ),
    to: '/docs/settings',
  },
  {
    title: 'Colors from the wallpaper',
    body: (
      <>
        One command picks a wallpaper and recolors the shell, the terminal and every app a matugen
        template reaches.
      </>
    ),
    to: '/docs/colors',
  },
  {
    title: 'Lock and session',
    body: (
      <>
        A session lock with the desktop blurred behind it, a polkit agent, a keyring prompt and a
        power menu.
      </>
    ),
    to: '/docs/lock',
  },
  {
    title: 'Scriptable',
    body: (
      <>
        Every panel answers a global shortcut, and scripts reach the shell through{' '}
        <code>proscenio ipc call</code>.
      </>
    ),
    to: '/docs/ipc',
  },
];

function Features(): ReactNode {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {features.map((feature) => (
            <div className="col col--3" key={feature.title}>
              <Link className={styles.feature} to={feature.to}>
                <h3>{feature.title}</h3>
                <p>{feature.body}</p>
              </Link>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function Hero(): ReactNode {
  return (
    <header className={styles.hero}>
      <div className="container">
        <img className={styles.glyph} src={useBaseUrl('img/proscenio-glyph.svg')} alt="" />
        <h1 className={styles.title}>proscenio</h1>
        <p className={styles.tagline}>
          A desktop shell for Hyprland, written in Rust with GTK 4: the bar, the sidebar,
          notifications, the dock, the launcher, the lock screen and a settings app, in one process
          that takes its colors from the wallpaper.
        </p>
        <div className={styles.install}>
          <CodeBlock language="bash">cargo build --release</CodeBlock>
        </div>
        <div className={styles.buttons}>
          <Link className="button button--primary button--lg" to="/docs/installing">
            Get started
          </Link>
          <Link className="button button--secondary button--lg" to="/docs">
            Read the docs
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home(): ReactNode {
  const {siteConfig} = useDocusaurusContext();

  return (
    <Layout title={siteConfig.title} description={siteConfig.tagline}>
      <Hero />
      <main>
        <Features />
      </main>
    </Layout>
  );
}
