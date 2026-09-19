# Disaster Recovery

## Objectives

Example objectives are RPO 24 hours and RTO 60 minutes; operators must replace them with business requirements. Use provider snapshots or `scripts/backup.sh` for compressed logical backups, store them encrypted outside the primary failure domain, and retain checksums separately.

## Backup

```bash
bash scripts/backup.sh "$DATABASE_URL" ./backups
sha256sum --check ./backups/orchestrator-*.dump.sha256
```

The script omits ownership/privileges for portability and never writes credentials into the archive name.

## Restore drill

Create a disposable database whose name ends in `_drill`, then:

```bash
bash scripts/restore-drill.sh ./backups/orchestrator-TIMESTAMP.dump \
  "$DRILL_DATABASE_URL" orchestrator_drill
```

The suffix is an intentional destructive-action guard. The drill runs `pg_restore --clean --if-exists`, checks that the `dags` table is queryable, and must never target production.

## Verification checklist

1. Record archive checksum, source version, start/end time, and operator.
2. Restore into isolated infrastructure.
3. Verify migrations, table counts, constraints, indexes, and a sampled DAG snapshot.
4. Start one API and one worker against the restored database.
5. Submit a new safe DAG, kill the worker after claim, and verify lease recovery.
6. Record observed RPO/RTO and securely destroy the drill database.

