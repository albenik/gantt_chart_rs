use svg::node::element::{
    Group,
    Rectangle,
    Text,
};

struct StyledShapes {
    row_height: f32,
    title_width: f32,
    bar_corner_radius: f32,
}

impl StyledShapes {
    pub fn row(&self, title: &str, x: f32, y: f32, w: f32, h: f32, class: &str) -> Group {
        Group::new()
            .set("transform", format!("translate({x}, {y})").as_str())
            .add(
                Text::new(title)
                    .set("class", "item")
                    .set("y", y + self.row_height / 2.0),
            )
            .add(
                Rectangle::new()
                    .set("class", class)
                    .set("x", self.title_width)
                    .set("y", y)
                    .set("width", w)
                    .set("height", h)
                    .set("rx", self.bar_corner_radius)
                    .set("ry", self.bar_corner_radius),
            )
    }
}
