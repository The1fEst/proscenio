import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import CodeBlock from '@theme/CodeBlock';
import Layout from '@theme/Layout';
import type {ReactNode} from 'react';

import styles from './index.module.css';

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
    </Layout>
  );
}
