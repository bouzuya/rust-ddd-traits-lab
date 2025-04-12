trait Aggregate: Sized {
    type Id: Eq;
    type Version: Eq + Ord;

    fn id(&self) -> &Self::Id;
    fn version(&self) -> &Self::Version;
}

trait Repository {
    type Aggregate: Aggregate;
    type Error: std::error::Error;

    async fn find(
        &self,
        id: &<Self::Aggregate as Aggregate>::Id,
    ) -> Result<Option<Self::Aggregate>, Self::Error>;

    async fn store(
        &self,
        expected_version: Option<&<Self::Aggregate as Aggregate>::Version>,
        aggregate: &Self::Aggregate,
    ) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Eq, PartialEq)]
    struct AggregateId(String);

    #[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
    struct AggregateVersion(u16);

    struct AggregateImpl {
        id: AggregateId,
        version: AggregateVersion,
    }

    impl AggregateImpl {
        fn create() -> Self {
            Self {
                id: AggregateId("1".to_owned()),
                version: AggregateVersion(1),
            }
        }
    }

    impl Aggregate for AggregateImpl {
        type Id = AggregateId;
        type Version = AggregateVersion;

        fn id(&self) -> &Self::Id {
            &self.id
        }

        fn version(&self) -> &Self::Version {
            &self.version
        }
    }

    struct RepositoryImpl;

    impl Repository for RepositoryImpl {
        type Aggregate = AggregateImpl;
        type Error = std::io::Error;

        async fn find(
            &self,
            _id: &<Self::Aggregate as Aggregate>::Id,
        ) -> Result<Option<Self::Aggregate>, Self::Error> {
            unimplemented!()
        }

        async fn store(
            &self,
            _expected_version: Option<&<Self::Aggregate as Aggregate>::Version>,
            _aggregate: &Self::Aggregate,
        ) -> Result<(), Self::Error> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_aggregate() {
        let aggregate = AggregateImpl::create();
        assert_eq!(aggregate.id(), &AggregateId("1".to_owned()));
        assert_eq!(aggregate.version(), &AggregateVersion(1));
    }
}
