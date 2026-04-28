-- Migration: Add custom models folder path to settings
-- When NULL/empty, the default app_data_dir/models/ is used
ALTER TABLE settings ADD COLUMN models_folder TEXT;
