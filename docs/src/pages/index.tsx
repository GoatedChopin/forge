import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';

import styles from './index.module.css';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link
            className="button button--secondary button--lg"
            to="/docs/start/first-app">
            Get Started
          </Link>
          <Link
            className="button button--outline button--secondary button--lg"
            to="/docs">
            Read the Docs
          </Link>
        </div>
      </div>
    </header>
  );
}

type FeatureItem = {
  title: string;
  description: JSX.Element;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'PostgreSQL-Powered',
    description: (
      <>
        No Redis. No message queues. No separate services. Just PostgreSQL and your code.
        Get auth, jobs, crons, workflows, real-time subscriptions, and observability out of the box.
      </>
    ),
  },
  {
    title: 'Type-Safe End-to-End',
    description: (
      <>
        Write your backend in Rust with full type safety. TypeScript types are generated
        automatically for your frontend. No more runtime surprises.
      </>
    ),
  },
  {
    title: 'Real-Time Built-In',
    description: (
      <>
        Queries automatically become reactive subscriptions. Data syncs across
        all clients via Server-Sent Events. No extra setup required.
      </>
    ),
  },
  {
    title: 'Background Processing',
    description: (
      <>
        Background jobs with retry logic, cron schedules with catch-up, and durable
        workflows that survive server restarts. All with progress tracking.
      </>
    ),
  },
  {
    title: 'AI-Ready with MCP',
    description: (
      <>
        Expose backend functions as MCP tools with a single macro. LLM agents can call
        your queries and mutations directly, with the same auth and rate limiting.
      </>
    ),
  },
  {
    title: 'Ship in Hours',
    description: (
      <>
        One command to scaffold a full-stack app. One binary to deploy. Focus on
        your business logic, not infrastructure.
      </>
    ),
  },
];

function Feature({title, description}: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <div className="padding-horiz--md padding-vert--lg">
        <Heading as="h3">{title}</Heading>
        <p>{description}</p>
      </div>
    </div>
  );
}

function HomepageFeatures(): JSX.Element {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}

function CodeExample(): JSX.Element {
  return (
    <section className={styles.codeExample}>
      <div className="container">
        <div className="row">
          <div className="col col--12">
            <Heading as="h2" className="text--center margin-bottom--lg">
              Code First
            </Heading>
          </div>
        </div>
        <div className="row">
          <div className="col col--6">
            <Heading as="h4">Define your data</Heading>
            <pre className={styles.codeBlock}>
{`#[forge::model]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub completed: bool,
}`}
            </pre>
          </div>
          <div className="col col--6">
            <Heading as="h4">Write a query</Heading>
            <pre className={styles.codeBlock}>
{`#[forge::query]
pub async fn list_tasks(ctx: &QueryContext)
    -> Result<Vec<Task>> {
    sqlx::query_as("SELECT * FROM tasks")
        .fetch_all(ctx.db()).await
        .map_err(Into::into)
}`}
            </pre>
          </div>
        </div>
        <div className="row margin-top--lg">
          <div className="col col--12">
            <Heading as="h4">Use it in your frontend</Heading>
            <pre className={styles.codeBlock}>
{`<script lang="ts">
  import { listTasksStore$ } from '$lib/forge';
  const tasks = listTasksStore$({});  // Auto-updates!
</script>

{#each $tasks.data ?? [] as task}
  <div>{task.title}</div>
{/each}`}
            </pre>
          </div>
        </div>
      </div>
    </section>
  );
}

function CTASection(): JSX.Element {
  return (
    <section className={styles.ctaSection}>
      <div className="container">
        <div className="row">
          <div className="col col--12 text--center">
            <Heading as="h2">Ready to Build?</Heading>
            <p className="margin-bottom--lg">
              Create your first FORGE app in under a minute.
            </p>
            <pre className={styles.codeBlock}>
{`cargo install forgex
forge new my-app --demo
cd my-app && forge dev`}
            </pre>
            <div className={styles.buttons}>
              <Link
                className="button button--primary button--lg"
                to="/docs/start/first-app">
                Your First App
              </Link>
              <Link
                className="button button--outline button--primary button--lg"
                to="/docs/build/read-data">
                Build Guide
              </Link>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

export default function Home(): JSX.Element {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title={`${siteConfig.title} - Full-Stack Rust Framework`}
      description="Build full-stack apps in hours, not weeks. Database, API, real-time updates, background jobs, and durable workflows - all backed by PostgreSQL.">
      <HomepageHeader />
      <main>
        <HomepageFeatures />
        <CodeExample />
        <CTASection />
      </main>
    </Layout>
  );
}
