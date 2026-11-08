use crate::db::models::InstallRow;
use crate::db::DbPool;
use crate::domain::{now_unix, InstallRecord, InstallState, PackageId};
use crate::error::{AppError, AppResult};

pub async fn upsert(pool: &DbPool, rec: &InstallRecord) -> AppResult<()> {
    let row = InstallRow::from(rec);
    sqlx::query(
        "INSERT INTO installs (package_id, source, state, installed_version, target_version, \
         pinned, auto_update, last_update_check, last_updated) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(package_id) DO UPDATE SET source = excluded.source, state = excluded.state, \
         installed_version = excluded.installed_version, target_version = excluded.target_version, \
         pinned = excluded.pinned, auto_update = excluded.auto_update, \
         last_update_check = excluded.last_update_check, last_updated = excluded.last_updated",
    )
    .bind(row.package_id)
    .bind(row.source)
    .bind(row.state)
    .bind(row.installed_version)
    .bind(row.target_version)
    .bind(row.pinned)
    .bind(row.auto_update)
    .bind(row.last_update_check)
    .bind(row.last_updated)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get(pool: &DbPool, id: &PackageId) -> AppResult<Option<InstallRecord>> {
    sqlx::query_as::<_, InstallRow>("SELECT * FROM installs WHERE package_id = ?")
        .bind(id.as_str())
        .fetch_optional(pool)
        .await?
        .map(InstallRecord::try_from)
        .transpose()
}

pub async fn list(pool: &DbPool) -> AppResult<Vec<InstallRecord>> {
    let rows = sqlx::query_as::<_, InstallRow>("SELECT * FROM installs ORDER BY package_id")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(InstallRecord::try_from).collect()
}

pub async fn list_by_state(pool: &DbPool, state: InstallState) -> AppResult<Vec<InstallRecord>> {
    let rows = sqlx::query_as::<_, InstallRow>(
        "SELECT * FROM installs WHERE state = ? ORDER BY package_id",
    )
    .bind(state.to_string())
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(InstallRecord::try_from).collect()
}

/// Validates the transition, persists it, and returns the updated record.
/// Illegal transitions return `Err` and write nothing.
pub async fn set_state(
    pool: &DbPool,
    id: &PackageId,
    next: InstallState,
) -> AppResult<InstallRecord> {
    let mut rec = get(pool, id)
        .await?
        .ok_or_else(|| AppError::PackageNotFound(id.to_string()))?;
    rec.state = rec.state.transition(next)?;
    if next == InstallState::Installed {
        rec.last_updated = Some(now_unix());
    }
    upsert(pool, &rec).await?;
    Ok(rec)
}

pub async fn set_pinned(pool: &DbPool, id: &PackageId, pinned: bool) -> AppResult<()> {
    let res = sqlx::query("UPDATE installs SET pinned = ? WHERE package_id = ?")
        .bind(i64::from(pinned))
        .bind(id.as_str())
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::PackageNotFound(id.to_string()));
    }
    Ok(())
}

pub async fn delete(pool: &DbPool, id: &PackageId) -> AppResult<()> {
    sqlx::query("DELETE FROM installs WHERE package_id = ?")
        .bind(id.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::test_pool;
    use crate::domain::SourceKind;

    fn rec(pkg: &str) -> InstallRecord {
        InstallRecord::new(
            PackageId::new(SourceKind::WingetCommunity, pkg),
            SourceKind::WingetCommunity,
        )
    }

    #[tokio::test]
    async fn upsert_get_list() {
        let pool = test_pool().await;
        let id = rec("a").package_id;
        assert!(get(&pool, &id).await.unwrap().is_none());
        let mut a = rec("a");
        upsert(&pool, &a).await.unwrap();
        assert_eq!(get(&pool, &id).await.unwrap().unwrap(), a);
        a.installed_version = Some("1.0".into());
        a.last_update_check = Some(5);
        upsert(&pool, &a).await.unwrap();
        upsert(&pool, &rec("b")).await.unwrap();
        let all = list(&pool).await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], a);
    }

    #[tokio::test]
    async fn list_by_state_filters() {
        let pool = test_pool().await;
        let mut a = rec("a");
        a.state = InstallState::Installed;
        upsert(&pool, &a).await.unwrap();
        upsert(&pool, &rec("b")).await.unwrap();
        let r = list_by_state(&pool, InstallState::Installed).await.unwrap();
        assert_eq!(r, vec![a]);
        assert!(list_by_state(&pool, InstallState::Failed)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn set_state_legal_path_sets_last_updated() {
        let pool = test_pool().await;
        let r = rec("a");
        upsert(&pool, &r).await.unwrap();
        let r1 = set_state(&pool, &r.package_id, InstallState::Installing)
            .await
            .unwrap();
        assert_eq!(r1.state, InstallState::Installing);
        assert_eq!(r1.last_updated, None);
        let r2 = set_state(&pool, &r.package_id, InstallState::Installed)
            .await
            .unwrap();
        assert!(r2.last_updated.unwrap() > 1_700_000_000);
        let stored = get(&pool, &r.package_id).await.unwrap().unwrap();
        assert_eq!(stored, r2);
        // Installed -> Installing (update) is legal.
        let r3 = set_state(&pool, &r.package_id, InstallState::Installing)
            .await
            .unwrap();
        assert_eq!(r3.state, InstallState::Installing);
    }

    #[tokio::test]
    async fn set_state_illegal_leaves_state_unchanged() {
        let pool = test_pool().await;
        let mut r = rec("a");
        r.state = InstallState::Installed;
        upsert(&pool, &r).await.unwrap();
        let err = set_state(&pool, &r.package_id, InstallState::NotInstalled)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "unsupported");
        let stored = get(&pool, &r.package_id).await.unwrap().unwrap();
        assert_eq!(stored.state, InstallState::Installed);
        assert_eq!(stored.last_updated, None);
    }

    #[tokio::test]
    async fn set_state_missing_is_not_found() {
        let pool = test_pool().await;
        let id = PackageId::new(SourceKind::Scoop, "ghost");
        let err = set_state(&pool, &id, InstallState::Installing)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "package_not_found");
    }

    #[tokio::test]
    async fn pinned_and_delete() {
        let pool = test_pool().await;
        let r = rec("a");
        upsert(&pool, &r).await.unwrap();
        set_pinned(&pool, &r.package_id, true).await.unwrap();
        assert!(get(&pool, &r.package_id).await.unwrap().unwrap().pinned);
        set_pinned(&pool, &r.package_id, false).await.unwrap();
        assert!(!get(&pool, &r.package_id).await.unwrap().unwrap().pinned);
        let ghost = PackageId::new(SourceKind::Scoop, "ghost");
        assert_eq!(
            set_pinned(&pool, &ghost, true).await.unwrap_err().code(),
            "package_not_found"
        );
        delete(&pool, &r.package_id).await.unwrap();
        assert!(get(&pool, &r.package_id).await.unwrap().is_none());
        delete(&pool, &r.package_id).await.unwrap();
    }
}
