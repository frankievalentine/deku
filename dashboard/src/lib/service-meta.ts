import type { ManagedServiceKind } from './api';

export const SERVICE_KIND_LABELS: Record<ManagedServiceKind, string> = {
  postgres: 'Postgres',
  redis: 'Redis',
  mysql: 'MySQL',
  mariadb: 'MariaDB',
  mongodb: 'MongoDB',
};

export const SERVICE_KIND_DESCRIPTIONS: Record<ManagedServiceKind, string> = {
  postgres: 'Managed relational database instances with backup and restore controls.',
  redis: 'Managed Redis caches for ephemeral state, queues, and session workloads.',
  mysql: 'Managed MySQL instances for apps that need MySQL-compatible relational storage.',
  mariadb: 'Managed MariaDB instances, a drop-in MySQL-compatible engine with its own URL key.',
  mongodb: 'Managed MongoDB instances for document workloads, backed up as a gzipped archive.',
};
