use crate::model::Property;

/// The public MIoT enable-time-period contract uses HH:MM:SS-HH:MM:SS.
pub fn recognizes(property: &Property) -> bool {
    property.key == "enable-time-period" && property.format == "string"
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeRange {
    pub start: [u8;3],
    pub end: [u8;3],
}

impl TimeRange {
    pub fn parse(raw: &str) -> Option<Self> {
        let (start,end)=raw.split_once('-')?;
        fn clock(raw:&str)->Option<[u8;3]> {
            let parts:Vec<_>=raw.split(':').collect();
            if !(2..=3).contains(&parts.len()) { return None; }
            let mut time=[0u8;3];
            for (index,part) in parts.iter().enumerate() {
                if part.len()!=2 || !part.bytes().all(|byte|byte.is_ascii_digit()) { return None; }
                time[index]=part.parse().ok()?;
            }
            (time[0]<24 && time[1]<60 && time[2]<60).then_some(time)
        }
        Some(Self {start:clock(start)?,end:clock(end)?})
    }
    pub fn wire(&self)->String {
        format!("{:02}:{:02}:{:02}-{:02}:{:02}:{:02}",self.start[0],self.start[1],self.start[2],self.end[0],self.end[1],self.end[2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn midnight_and_overnight_ranges_keep_their_original_times() {
        let period=TimeRange::parse("22:00-06:00").unwrap();
        assert_eq!(period.wire(),"22:00:00-06:00:00");
        assert_eq!(TimeRange::parse(&period.wire()),Some(period));
        let seconds=TimeRange::parse("23:59:37-00:01:09").unwrap();
        assert_eq!(seconds.start[2],37);
        assert_eq!(seconds.wire(),"23:59:37-00:01:09");
        assert!(TimeRange::parse("00:00:00-00:00:00").is_some());
    }
    #[test]
    fn invalid_or_unrelated_strings_cannot_become_a_schedule() {
        for raw in ["24:00-06:00","22:60-06:00","22:00:60-06:00:00","2:00-6:00","22:00-06:00-extra","","22:00","22:00--06:00","22:00:00:00-06:00"] {assert!(TimeRange::parse(raw).is_none(),"{raw}");}
        assert!(!recognizes(&Property {key:"vendor-string".into(),format:"string".into(),..Default::default()}));
    }
}
