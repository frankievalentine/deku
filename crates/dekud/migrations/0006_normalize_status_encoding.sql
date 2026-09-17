UPDATE apps SET status = lower(status);

UPDATE deployments SET builder = lower(builder);

UPDATE deployments SET status = CASE status
  WHEN 'HealthChecking' THEN 'health_checking'
  WHEN 'RolledBack' THEN 'rolled_back'
  ELSE lower(status)
END;
